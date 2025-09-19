//! solver3/fc/mod.rs
//!
//! Formatting context managers for different CSS display types

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    app_resources::RendererResources,
    dom::NodeId,
    styled_dom::StyledDom,
    ui_solver::FormattingContext,
    window::{LogicalPosition, LogicalRect, LogicalSize, WritingMode},
};
use azul_css::LayoutDebugMessage;
use usvg::Text;

use crate::{
    solver3::{
        geometry::{BoxProps, Clear, DisplayType, EdgeSizes, Float},
        layout_tree::{LayoutNode, LayoutTree},
        positioning::PositionType,
        sizing::extract_text_from_node,
        LayoutContext, LayoutError, Result,
    },
    text3::{
        self,
        cache::{
            FontLoaderTrait, InlineContent, InlineShape, LayoutFragment, ParsedFontTrait,
            ShapeDefinition, Size, StyleProperties, StyledRun, UnifiedConstraints,
        },
    },
};

/// Input constraints for a layout function.
#[derive(Debug)]
pub struct LayoutConstraints<'a> {
    /// The available space for the content, excluding padding and borders.
    pub available_size: LogicalSize,
    /// The CSS writing-mode of the context.
    pub writing_mode: WritingMode,
    /// The state of the parent Block Formatting Context, if applicable.
    /// This is how state (like floats) is passed down.
    pub bfc_state: Option<&'a mut BfcState>,
    // Other properties like text-align would go here.
    pub text_align: TextAlign,
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
#[derive(Debug, Default)]
pub struct LayoutOutput {
    /// The final positions of child nodes, relative to the container's content-box origin.
    pub positions: BTreeMap<usize, LogicalPosition>,
    /// The total size occupied by the content, which may exceed `available_size`.
    pub overflow_size: LogicalSize,
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
    kind: Float,
    /// The rectangle occupied by the float's margin-box.
    rect: LogicalRect,
}

/// Manages the state of all floated elements within a Block Formatting Context.
#[derive(Debug, Default, Clone)]
pub struct FloatingContext {
    floats: Vec<FloatBox>,
}

impl FloatingContext {
    /// Returns the main-axis offset needed to be clear of floats of the given type.
    /// In horizontal writing-mode, this is the Y coordinate to move to.
    pub fn clearance_offset(&self, clear: Clear, current_main_offset: f32) -> f32 {
        let mut max_end_offset = 0.0_f32;

        // Determine which floats to check based on the `clear` property.
        let check_left = clear == Clear::Left || clear == Clear::Both;
        let check_right = clear == Clear::Right || clear == Clear::Both;

        for float in &self.floats {
            let should_clear_this_float = (check_left && float.kind == Float::Left)
                || (check_right && float.kind == Float::Right);

            if should_clear_this_float {
                // The "clearance" position is the bottom edge of the float.
                // We find the maximum bottom edge among all relevant floats.
                let float_bottom = float.rect.origin.y + float.rect.size.height;
                max_end_offset = max_end_offset.max(float_bottom);
            }
        }

        // Only return a new offset if it's greater than the current one.
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
    writing_mode: WritingMode,
}

/// Result of a formatting context layout operation
#[derive(Debug, Default)]
pub struct LayoutResult {
    pub positions: Vec<(usize, LogicalPosition)>,
    pub overflow_size: Option<LogicalSize>,
    pub baseline_offset: f32,
}

/// Main dispatcher for formatting context layout.
pub fn layout_formatting_context<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    match node.formatting_context {
        FormattingContext::Block { .. } => layout_bfc(ctx, tree, node_index, constraints),
        FormattingContext::Inline => layout_ifc(ctx, text_cache, tree, node_index, constraints),
        FormattingContext::Table => layout_table_fc(ctx, tree, node_index, constraints),
        _ => layout_bfc(ctx, tree, node_index, constraints),
    }
}

/// Lays out a Block Formatting Context (BFC).
/// This function correctly handles different writing modes by operating on
/// logical main (block) and cross (inline) axes.
fn layout_bfc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    let node = tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?
        .clone(); // Clone to satisfy borrow checker

    // Use the writing mode from the layout constraints.
    let writing_mode = constraints.writing_mode;

    let mut output = LayoutOutput::default();
    let mut bfc_state = BfcState::new();

    // Logical axis tracking:
    // - `main_pen`: The current position along the main/block axis (e.g., 'y' in horizontal-tb).
    // - `max_cross_size`: The maximum size encountered on the cross/inline axis (e.g., 'width').
    let mut main_pen = 0.0_f32;
    let mut max_cross_size = 0.0_f32;

    for &child_index in &node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        // Determine if the child is in-flow for this formatting context.
        let position_type = get_position_type(ctx.styled_dom, child_dom_id);
        let is_in_flow =
            position_type == PositionType::Static || position_type == PositionType::Relative;

        // Get the child's already-calculated size and margins.
        let child_size = child_node.used_size.unwrap_or_default();
        let child_margin = &child_node.box_props.margin;

        // --- Static Position Calculation (using logical axes) ---

        // The child's position on the main axis is the current pen plus its start margin.
        let static_main_pos = main_pen + child_margin.main_start(writing_mode); // Simplified margin collapsing

        // The child's position on the cross axis depends on alignment (defaulting to start).
        // A full implementation would use text-align/justify-content here.
        let static_cross_pos = child_margin.cross_start(writing_mode);

        // Convert the logical main/cross position to a physical x/y position.
        let static_pos =
            LogicalPosition::from_main_cross(static_main_pos, static_cross_pos, writing_mode);
        output.positions.insert(child_index, static_pos);

        // --- Pen Advancement and Size Tracking (for in-flow elements only) ---
        if is_in_flow {
            // Check if the child establishes an Inline Formatting Context.
            if child_node.formatting_context == FormattingContext::Inline {
                // Inline content is special. We use text3 to lay it out, and it
                // produces a single block-level "anonymous box" for the BFC to stack.
                // The IFC's height (or width in vertical mode) determines how much to advance the
                // pen.

                // (This part assumes that the IFC has already been sized during the intrinsic
                // sizing pass and its final size is available in `child_size`).

                let ifc_block_size = child_size.main(writing_mode);
                main_pen += ifc_block_size; // Advance pen by the IFC's total block size.
                max_cross_size = max_cross_size.max(child_size.cross(writing_mode));
            } else {
                // This is a standard block-level child.

                // Calculate its full margin-box size on the main axis.
                let margin_box_main_size =
                    child_size.main(writing_mode) + child_margin.main_sum(writing_mode);

                // Advance the pen by the child's full margin-box size.
                main_pen += margin_box_main_size;

                // Update the max cross size for the BFC.
                let margin_box_cross_size =
                    child_size.cross(writing_mode) + child_margin.cross_sum(writing_mode);
                max_cross_size = max_cross_size.max(margin_box_cross_size);
            }
        }
    }

    // Convert the final logical main/cross overflow size to a physical width/height.
    output.overflow_size = LogicalSize::from_main_cross(main_pen, max_cross_size, writing_mode);
    Ok(output)
}

/// Lays out an Inline Formatting Context (IFC).
fn layout_ifc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    tree: &mut LayoutTree<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    // 1. Collect all inline content (text runs, inline-blocks) from descendants.
    // This is a complex traversal that needs to be implemented.
    let inline_content = collect_inline_content(ctx, tree, node_index)?;

    // 2. Prepare constraints for text3, including float exclusion zones.
    let text3_constraints = UnifiedConstraints {
        available_width: constraints.available_size.width,
        available_height: Some(constraints.available_size.height),
        // TODO: Convert `FloatingContext` into a set of exclusion rectangles for text3.
        // exclusion_zones: constraints.floats.to_exclusion_zones(),
        ..Default::default()
    };
    let fragments = vec![LayoutFragment {
        id: "main".to_string(),
        constraints: text3_constraints,
    }];

    // 3. Call text3 to perform the inline layout.
    let text_layout_result =
        text_cache.layout_flow(&inline_content, &[], &fragments, ctx.font_manager)?;

    // 4. Store the detailed text layout result on the tree node for display list generation.
    let mut output = LayoutOutput::default();
    if let Some(node) = tree.get_mut(node_index) {
        if let Some(main_frag) = text_layout_result.fragment_layouts.get("main") {
            node.inline_layout_result = Some(main_frag.clone());

            // 5. Convert the text3 result back into LayoutOutput.
            output.overflow_size =
                LogicalSize::new(main_frag.bounds.width, main_frag.bounds.height);
            // `output.positions` would be empty as text3 handles internal positioning.
        }
    }

    Ok(output)
}

/// Lays out a Table Formatting Context.
fn layout_table_fc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    ctx.debug_log("Laying out table (STUB)");
    // A real implementation would be a multi-pass algorithm:
    // 1. Determine number of columns and create a grid structure.
    // 2. Calculate min/max content width for each cell.
    // 3. Resolve column widths based on table width and cell constraints.
    // 4. Layout cells within their final column widths to determine row heights.
    // 5. Position cells within the final grid.

    // For now, we fall back to simple block stacking.
    layout_bfc(ctx, tree, node_index, constraints)
}

/// Helper function to gather all inline content for text3, including inline-blocks.
pub fn collect_inline_content<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    tree: &LayoutTree<T>,
    ifc_root_index: usize,
) -> Result<Vec<InlineContent>> {
    let mut content = Vec::new();
    let ifc_root_node = tree.get(ifc_root_index).ok_or(LayoutError::InvalidTree)?;

    for &child_index in &ifc_root_node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let Some(dom_id) = child_node.dom_node_id else {
            continue;
        };

        if get_display_property(ctx.styled_dom, Some(dom_id)) == DisplayType::InlineBlock {
            let size = child_node.used_size.unwrap_or_default();

            // TODO: Calculate the real baseline of the inline-block element.
            // This would involve running layout on its children and finding the baseline
            // of its last line box. For now, we stub it as the bottom of the box.
            let baseline_offset = size.height;
            content.push(InlineContent::Shape(InlineShape {
                shape_def: ShapeDefinition::Rectangle {
                    size: Size {
                        width: size.width,
                        height: size.height,
                    },
                    corner_radius: None,
                },
                fill: None,
                stroke: None,
                baseline_offset,
            }));
        } else {
            // Otherwise, assume it's text or another standard inline element.
            if let Some(text) = extract_text_from_node(ctx.styled_dom, dom_id) {
                content.push(InlineContent::Text(StyledRun {
                    text,
                    style: Arc::new(get_style_properties(ctx.styled_dom, dom_id)), // STUB
                    logical_start_byte: 0,
                }));
            }
        }
    }
    Ok(content)
}

// TODO: STUB helper functions that would be needed for the above code.
fn get_display_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> DisplayType {
    // In a real implementation, this would read the 'display' property
    DisplayType::Inline // Default
}

// TODO: STUB helper
fn get_style_properties(styled_dom: &StyledDom, dom_id: NodeId) -> StyleProperties {
    // In a real implementation, this would convert CSS props to text3 StyleProperties
    StyleProperties::default()
}

/// Positions a floated child within the BFC and updates the floating context.
///
/// This function implements the complex logic of finding the available space for a new float
/// at the highest possible vertical position.
fn position_floated_child(
    child_index: usize,
    child_size: LogicalSize, // The margin-box size of the child
    float_type: Float,
    constraints: &LayoutConstraints,
    bfc_content_box: LogicalRect, // The content box of the BFC
    current_main_offset: f32,     // The current line's vertical position
    floating_context: &mut FloatingContext,
) -> Result<LogicalPosition> {
    let mut placement_y = current_main_offset;

    // Loop to find the highest vertical position where the float fits.
    // This is the core of float placement logic.
    loop {
        // 1. Determine the available horizontal space at the current `placement_y`.
        let mut available_left_edge = bfc_content_box.origin.x;
        let mut available_right_edge = bfc_content_box.origin.x + bfc_content_box.size.width;

        for existing_float in &floating_context.floats {
            // If the existing float vertically overlaps with the line we are trying to place on...
            if placement_y < existing_float.rect.origin.y + existing_float.rect.size.height
                && existing_float.rect.origin.y < placement_y + child_size.height
            {
                if existing_float.kind == Float::Left {
                    available_left_edge = available_left_edge
                        .max(existing_float.rect.origin.x + existing_float.rect.size.width);
                } else {
                    // Float::Right
                    available_right_edge = available_right_edge.min(existing_float.rect.origin.x);
                }
            }
        }

        let available_width = available_right_edge - available_left_edge;

        // 2. Check if the new float can fit in the available space.
        if child_size.width <= available_width {
            // It fits! Determine the final position and add it to the context.
            let final_pos = match float_type {
                Float::Left => LogicalPosition::new(available_left_edge, placement_y),
                Float::Right => {
                    LogicalPosition::new(available_right_edge - child_size.width, placement_y)
                }
                Float::None => unreachable!(),
            };

            let new_float_box = FloatBox {
                kind: float_type,
                // The rectangle must be relative to the BFC's coordinate system
                rect: LogicalRect::new(final_pos, child_size),
            };

            floating_context.floats.push(new_float_box);

            return Ok(final_pos);
        } else {
            // It doesn't fit. We must move the float down past the obstacle.
            // Find the lowest bottom-edge of all floats that are blocking the current line.
            let mut next_y = f32::INFINITY;
            for existing_float in &floating_context.floats {
                if placement_y < existing_float.rect.origin.y + existing_float.rect.size.height {
                    if existing_float.kind == Float::Left
                        && available_left_edge
                            <= existing_float.rect.origin.x + existing_float.rect.size.width
                    {
                        next_y = next_y
                            .min(existing_float.rect.origin.y + existing_float.rect.size.height);
                    }
                    if existing_float.kind == Float::Right
                        && available_right_edge >= existing_float.rect.origin.x
                    {
                        next_y = next_y
                            .min(existing_float.rect.origin.y + existing_float.rect.size.height);
                    }
                }
            }

            if next_y.is_infinite() {
                // Should not happen if width is > 0 and a float is actually blocking
                return Err(LayoutError::PositioningFailed);
            }
            placement_y = next_y;
        }
    }
}

/// Adjusts the BFC pen position to clear floats. Returns true if clearance was applied.
fn apply_clearance(child_index: usize, state: &mut BfcLayoutState) -> bool {
    let clear_y = 0.0; // Placeholder for calculated clearance value
                       // In a real implementation:
                       // let clear_prop = get_clear_property(...);
                       // let clear_y = state.floats.get_clearance_y(clear_prop);

    if clear_y > state.pen.y {
        state.pen.y = clear_y;
        // When clearance is applied, margin collapsing is suppressed.
        state.margins.last_in_flow_margin_bottom = 0.0;
        true
    } else {
        false
    }
}

// STUB: Functions to get CSS properties
fn get_float_property(tree: &StyledDom, dom_id: Option<azul_core::dom::NodeId>) -> Float {
    Float::None
}
fn get_clear_property(tree: &StyledDom, dom_id: Option<azul_core::dom::NodeId>) -> Clear {
    Clear::None
}
fn get_text_align(tree: &StyledDom, dom_id: Option<azul_core::dom::NodeId>) -> TextAlign {
    TextAlign::Start
}

/// Helper to determine if scrollbars are needed
pub fn check_scrollbar_necessity(
    content_size: LogicalSize,
    container_size: LogicalSize,
    overflow_x: OverflowBehavior,
    overflow_y: OverflowBehavior,
) -> ScrollbarInfo {
    let needs_horizontal = match overflow_x {
        OverflowBehavior::Visible | OverflowBehavior::Hidden => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.width > container_size.width,
    };

    let needs_vertical = match overflow_y {
        OverflowBehavior::Visible | OverflowBehavior::Hidden => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.height > container_size.height,
    };

    ScrollbarInfo {
        needs_horizontal,
        needs_vertical,
        scrollbar_width: if needs_vertical { 16.0 } else { 0.0 },
        scrollbar_height: if needs_horizontal { 16.0 } else { 0.0 },
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OverflowBehavior {
    Visible,
    Hidden,
    Scroll,
    Auto,
}

#[derive(Debug, Clone)]
pub struct ScrollbarInfo {
    pub needs_horizontal: bool,
    pub needs_vertical: bool,
    pub scrollbar_width: f32,
    pub scrollbar_height: f32,
}

impl ScrollbarInfo {
    /// Checks if the presence of scrollbars reduces the available inner size,
    /// which would necessitate a reflow of the content.
    pub fn needs_reflow(&self) -> bool {
        self.scrollbar_width > 0.0 || self.scrollbar_height > 0.0
    }

    /// Takes a size (representing a content-box) and returns a new size
    /// reduced by the dimensions of any active scrollbars.
    pub fn shrink_size(&self, size: LogicalSize) -> LogicalSize {
        LogicalSize {
            width: (size.width - self.scrollbar_width).max(0.0),
            height: (size.height - self.scrollbar_height).max(0.0),
        }
    }
}

/// Margin collapsing calculation for block layout
pub fn calculate_collapsed_margins(top_margin: f32, bottom_margin: f32, is_adjacent: bool) -> f32 {
    if !is_adjacent {
        return 0.0;
    }

    // Simplified margin collapsing - real implementation would be more complex
    if top_margin.signum() == bottom_margin.signum() {
        top_margin.abs().max(bottom_margin.abs()) * top_margin.signum()
    } else {
        top_margin + bottom_margin
    }
}

fn debug_log(debug_messages: &mut Option<Vec<LayoutDebugMessage>>, message: &str) {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: message.into(),
            location: "formatting_contexts".into(),
        });
    }
}
