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
use azul_css::{CssProperty, CssPropertyValue, LayoutDebugMessage, LayoutFloat};
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
            FontLoaderTrait, InlineContent, InlineShape, LayoutCache as TextLayoutCache,
            LayoutFragment, OverflowBehavior, ParsedFontTrait, ShapeDefinition, Size,
            StyleProperties, StyledRun, UnifiedConstraints,
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
    /// Finds the available space on the cross-axis for a line box at a given main-axis range.
    ///
    /// Returns a tuple of (`cross_start_offset`, `cross_end_offset`) relative to the
    /// BFC content box, defining the available space for an in-flow element.
    pub fn available_line_box_space(
        &self,
        main_start: f32,
        main_end: f32,
        bfc_cross_size: f32,
        wm: WritingMode,
    ) -> (f32, f32) {
        let mut available_cross_start = 0.0_f32;
        let mut available_cross_end = bfc_cross_size;

        for float in &self.floats {
            // Get the logical main-axis span of the existing float.
            let float_main_start = float.rect.origin.main(wm);
            let float_main_end = float_main_start + float.rect.size.main(wm);

            // Check for overlap on the main axis.
            if main_end > float_main_start && main_start < float_main_end {
                // The float overlaps with the main-axis range of the element we're placing.
                let float_cross_start = float.rect.origin.cross(wm);
                let float_cross_end = float_cross_start + float.rect.size.cross(wm);

                if float.kind == Float::Left {
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

    /// Returns the main-axis offset needed to be clear of floats of the given type.
    pub fn clearance_offset(&self, clear: Clear, current_main_offset: f32, wm: WritingMode) -> f32 {
        let mut max_end_offset = 0.0_f32;

        let check_left = clear == Clear::Left || clear == Clear::Both;
        let check_right = clear == Clear::Right || clear == Clear::Both;

        for float in &self.floats {
            let should_clear_this_float = (check_left && float.kind == Float::Left)
                || (check_right && float.kind == Float::Right);

            if should_clear_this_float {
                let float_main_end = float.rect.origin.main(wm) + float.rect.size.main(wm);
                max_end_offset = max_end_offset.max(float_main_end);
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
///
/// This function correctly handles different writing modes by operating on
/// logical main (block) and cross (inline) axes. It also correctly implements
/// vertical margin collapsing between in-flow block-level children.
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

    let writing_mode = constraints.writing_mode;

    let mut output = LayoutOutput::default();
    let mut bfc_state = BfcState::new();
    let mut last_in_flow_child_idx = None;

    // The main_pen tracks the bottom edge of the *border-box* of the last
    // in-flow, non-cleared block-level element.
    let mut main_pen = 0.0_f32;
    let mut max_cross_size = 0.0_f32;

    for &child_index in &node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        let position_type = get_position_type(ctx.styled_dom, child_dom_id);
        if position_type == PositionType::Absolute || position_type == PositionType::Fixed {
            continue; // Out-of-flow elements are handled in a separate pass.
        }

        let float_type = get_float_property(ctx.styled_dom, child_dom_id);
        let clear_type = get_clear_property(ctx.styled_dom, child_dom_id);
        let child_size = child_node.used_size.unwrap_or_default(); // This is border-box size
        let child_margin = &child_node.box_props.margin;

        if float_type != Float::None {
            // Floated elements are taken out of the normal flow.
            let margin_box_size = LogicalSize::new(
                child_size.width + child_margin.cross_sum(writing_mode),
                child_size.height + child_margin.main_sum(writing_mode),
            );
            let bfc_content_box =
                LogicalRect::new(LogicalPosition::zero(), constraints.available_size);
            let float_pos = position_floated_child(
                child_index,
                margin_box_size,
                float_type,
                constraints,
                bfc_content_box,
                main_pen, // Floats are placed relative to the current flow position
                &mut bfc_state.floats,
            )?;
            output.positions.insert(child_index, float_pos);
        } else {
            // This is an in-flow, non-floated block-level element.
            let border_box_main_size = child_size.main(writing_mode);
            let top_margin = child_margin.main_start(writing_mode);
            let bottom_margin = child_margin.main_end(writing_mode);

            // 1. Handle clearance.
            // The "current vertical position" is the bottom of the previous margin box.
            let flow_bottom = main_pen + bfc_state.margins.last_in_flow_margin_bottom;
            let clear_pen =
                bfc_state
                    .floats
                    .clearance_offset(clear_type, flow_bottom, writing_mode);

            let mut static_main_pos;

            if clear_pen > flow_bottom {
                // Clearance is applied. This creates a hard separation.
                // The top of the new element's MARGIN box is now at `clear_pen`.
                static_main_pos = clear_pen + top_margin;
                // The previous margin does not collapse across a clearance.
                bfc_state.margins.last_in_flow_margin_bottom = 0.0;
            } else {
                // 2. No clearance, perform margin collapsing.
                let prev_margin = bfc_state.margins.last_in_flow_margin_bottom;
                let collapsed_margin_space = collapse_margins(prev_margin, top_margin);

                // The element's top border edge is positioned relative to the previous
                // element's bottom border edge (`main_pen`) plus the collapsed margin.
                static_main_pos = main_pen + collapsed_margin_space;
            }

            // 3. Find available cross-axis space at this position, considering floats.
            let bfc_cross_size = constraints.available_size.cross(writing_mode);
            let (line_box_cross_start, _line_box_cross_end) =
                bfc_state.floats.available_line_box_space(
                    static_main_pos,
                    static_main_pos + border_box_main_size,
                    bfc_cross_size,
                    writing_mode,
                );

            // 4. Set the final position for this child.
            let static_cross_pos = line_box_cross_start + child_margin.cross_start(writing_mode);
            let static_pos =
                LogicalPosition::from_main_cross(static_main_pos, static_cross_pos, writing_mode);
            output.positions.insert(child_index, static_pos);

            // 5. Update state for the next iteration.
            main_pen = static_main_pos + border_box_main_size;
            bfc_state.margins.last_in_flow_margin_bottom = bottom_margin;
            last_in_flow_child_idx = Some(child_index);

            // 6. Update BFC cross-axis extent.
            let child_extent_cross = static_cross_pos
                + child_size.cross(writing_mode)
                + child_margin.cross_end(writing_mode);
            max_cross_size = max_cross_size.max(child_extent_cross);
        }
    }

    // The final BFC main size is determined by the position of the last element's
    // bottom margin, which may or may not collapse with the parent's bottom margin.
    // For calculating overflow, we include this last margin.
    let final_content_main_size = main_pen + bfc_state.margins.last_in_flow_margin_bottom;

    // The final size must also be large enough to contain the bottom of all floats.
    let float_main_end = bfc_state
        .floats
        .floats
        .iter()
        .map(|f| f.rect.origin.main(writing_mode) + f.rect.size.main(writing_mode))
        .fold(0.0, f32::max);

    main_pen = final_content_main_size.max(float_main_end);

    // The final BFC cross size must also encompass any floats.
    for float in &bfc_state.floats.floats {
        let float_extent_cross =
            float.rect.origin.cross(writing_mode) + float.rect.size.cross(writing_mode);
        max_cross_size = max_cross_size.max(float_extent_cross);
    }

    output.overflow_size = LogicalSize::from_main_cross(main_pen, max_cross_size, writing_mode);

    // --- Baseline Calculation ---
    // The baseline of a BFC is the baseline of its last in-flow child that has a baseline.
    if let Some(last_child_idx) = last_in_flow_child_idx {
        if let (Some(last_child_node), Some(last_child_pos)) = (
            tree.get(last_child_idx),
            output.positions.get(&last_child_idx),
        ) {
            if let Some(child_baseline) = last_child_node.baseline {
                // The child's baseline is relative to its own content-box top edge.
                let border_box_top = last_child_pos.main(writing_mode);
                let content_box_top = border_box_top
                    + last_child_node.box_props.padding.main_start(writing_mode)
                    + last_child_node.box_props.border.main_start(writing_mode);
                output.baseline = Some(content_box_top + child_baseline);
            }
        }
    }

    let node = tree.get_mut(node_index).unwrap();
    node.baseline = output.baseline;

    Ok(output)
}

/// Lays out an Inline FormattingContext (IFC).
fn layout_ifc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    tree: &mut LayoutTree<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    // 1. Collect all inline content (text runs, inline-blocks) from descendants.
    let inline_content = collect_inline_content(ctx, text_cache, tree, node_index)?;

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
            // The baseline of an IFC is the baseline of its last line box.
            output.baseline = main_frag.last_baseline;
            node.baseline = output.baseline;
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
/// Helper function to gather all inline content for text3, including inline-blocks.
pub fn collect_inline_content<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    text_cache: &mut TextLayoutCache<T>,
    tree: &mut LayoutTree<T>,
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
            let baseline_offset = get_or_calculate_baseline(ctx, text_cache, tree, child_index)?
                .unwrap_or(size.height);

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

/// Gets the baseline for a node, calculating and caching it if necessary.
/// The baseline of an inline-block is the baseline of its last line box.
fn get_or_calculate_baseline<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    text_cache: &mut TextLayoutCache<T>,
    tree: &mut LayoutTree<T>,
    node_index: usize,
) -> Result<Option<f32>> {
    // Check cache first
    if let Some(baseline) = tree.get(node_index).unwrap().baseline {
        return Ok(Some(baseline));
    }

    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
    let used_size = node.used_size.unwrap_or_default();
    let writing_mode = get_writing_mode(ctx.styled_dom, node.dom_node_id);

    // To find the baseline, we must lay out the node's contents.
    // Create temporary constraints based on its already-calculated used size.
    let constraints = LayoutConstraints {
        available_size: node.box_props.inner_size(used_size, writing_mode),
        bfc_state: None,
        writing_mode,
        text_align: TextAlign::Start, // Does not affect baseline
    };

    // Temporarily mutate the context to avoid borrowing issues
    let mut temp_ctx = LayoutContext {
        styled_dom: ctx.styled_dom,
        font_manager: ctx.font_manager,
        debug_messages: &mut None, // Discard debug messages from this temporary layout
    };

    let layout_output =
        layout_formatting_context(&mut temp_ctx, tree, text_cache, node_index, &constraints)?;

    // Cache the result on the node
    let baseline = layout_output.baseline;
    tree.get_mut(node_index).unwrap().baseline = baseline;

    Ok(baseline)
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
/// This function is fully writing-mode aware.
fn position_floated_child<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    _child_index: usize,
    child_margin_box_size: LogicalSize,
    float_type: Float,
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
        // 1. Determine the available cross-axis space at the current `placement_main_offset`.
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
            let final_cross_pos = match float_type {
                Float::Left => available_cross_start,
                Float::Right => available_cross_end - child_cross_size,
                Float::None => unreachable!(),
            };
            let final_pos =
                LogicalPosition::from_main_cross(placement_main_offset, final_cross_pos, wm);

            let new_float_box = FloatBox {
                kind: float_type,
                rect: LogicalRect::new(final_pos, child_margin_box_size),
            };
            floating_context.floats.push(new_float_box);
            return Ok(final_pos);
        } else {
            // It doesn't fit. We must move the float down past an obstacle.
            // Find the lowest main-axis end of all floats that are blocking the current line.
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
                // This indicates an unrecoverable state, e.g., a float wider than the container.
                return Err(LayoutError::PositioningFailed);
            }
            placement_main_offset = next_main_offset;
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
fn get_float_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> Float {
    let Some(id) = dom_id else {
        return Float::None;
    };
    if let Some(styled_node) = styled_dom.styled_nodes.as_container().get(id) {
        if let Some(CssProperty::Float(CssPropertyValue::Exact(float))) =
            styled_node.state.get_style().get(&CssProperty::Float)
        {
            return match float {
                LayoutFloat::Left => Float::Left,
                LayoutFloat::Right => Float::Right,
                LayoutFloat::None => Float::None,
            };
        }
    }
    Float::None
}

fn get_clear_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> Clear {
    let Some(id) = dom_id else {
        return Clear::None;
    };
    if let Some(styled_node) = styled_dom.styled_nodes.as_container().get(id) {
        if let Some(CssProperty::Clear(CssPropertyValue::Exact(clear))) =
            styled_node.state.get_style().get(&CssProperty::Clear)
        {
            return match clear {
                LayoutClear::Left => Clear::Left,
                LayoutClear::Right => Clear::Right,
                LayoutClear::Both => Clear::Both,
                LayoutClear::None => Clear::None,
            };
        }
    }
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
        OverflowBehavior::Break => false, // TODO: ???
    };

    let needs_vertical = match overflow_y {
        OverflowBehavior::Visible | OverflowBehavior::Hidden => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.height > container_size.height,
        OverflowBehavior::Break => false, // TODO: ???
    };

    ScrollbarInfo {
        needs_horizontal,
        needs_vertical,
        scrollbar_width: if needs_vertical { 16.0 } else { 0.0 },
        scrollbar_height: if needs_horizontal { 16.0 } else { 0.0 },
    }
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

/// Calculates a single collapsed margin from two adjoining vertical margins.
///
/// Implements the rules from CSS 2.1 section 8.3.1:
/// - If both margins are positive, the result is the larger of the two.
/// - If both margins are negative, the result is the more negative of the two.
/// - If the margins have mixed signs, they are effectively summed.
fn collapse_margins(a: f32, b: f32) -> f32 {
    if a.is_sign_positive() && b.is_sign_positive() {
        a.max(b)
    } else if a.is_sign_negative() && b.is_sign_negative() {
        a.min(b)
    } else {
        a + b
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
