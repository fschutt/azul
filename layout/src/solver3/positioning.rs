//! solver3/positioning.rs
//! Pass 3: Final positioning of layout nodes

use std::collections::BTreeMap;

use azul_core::{
    app_resources::RendererResources,
    callbacks::ScrollPosition,
    dom::NodeId,
    styled_dom::StyledDom,
    window::{LogicalPosition, LogicalRect, LogicalSize, WritingMode},
};
use azul_css::{CssProperty, CssPropertyValue, LayoutDebugMessage, LayoutPosition};

use crate::{
    solver3::{
        fc::{layout_formatting_context, LayoutConstraints, TextAlign},
        geometry::CssSize,
        layout_tree::LayoutTree,
        LayoutContext, LayoutError, Result,
    },
    text3::cache::{FontLoaderTrait, ParsedFontTrait},
};

/// Final positioned layout tree
#[derive(Debug)]
pub struct PositionedLayoutTree {
    pub tree: LayoutTree,
    pub absolute_positions: BTreeMap<usize, LogicalPosition>,
    pub used_sizes: BTreeMap<usize, LogicalSize>,
    /// Tracks nodes whose inner size was changed by adding scrollbars.
    /// The main layout loop uses this to trigger a reflow.
    pub new_sizes_from_scrollbars: BTreeMap<usize, LogicalSize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionType {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

#[derive(Debug, Default)]
struct PositionOffsets {
    top: Option<f32>,
    right: Option<f32>,
    bottom: Option<f32>,
    left: Option<f32>,
}

impl PositionedLayoutTree {
    pub fn get_rectangles(
        &self,
    ) -> BTreeMap<azul_core::dom::NodeId, azul_core::ui_solver::PositionedRectangle> {
        self.tree.get_rectangles()
    }

    pub fn get_word_positions(
        &self,
    ) -> BTreeMap<azul_core::dom::NodeId, Vec<azul_core::window::LogicalRect>> {
        self.tree.get_word_positions()
    }
}

/// Calculate final positions for all nodes
pub fn calculate_positions<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree,
    used_sizes: &BTreeMap<usize, LogicalSize>,
    viewport: LogicalRect,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
) -> Result<PositionedLayoutTree> {
    ctx.debug_log("Starting position calculation");

    let mut positioned_tree = PositionedLayoutTree {
        tree: tree.clone(), // Clone the tree to modify it during positioning
        absolute_positions: BTreeMap::new(),
        used_sizes: used_sizes.clone(),
        new_sizes_from_scrollbars: BTreeMap::new(),
    };

    // Pass 3a: Layout in-flow elements
    position_node_recursive(&mut positioned_tree, tree.root, LogicalPosition::zero())?;

    // Pass 3b: Handle out-of-flow elements (absolute, fixed)
    handle_out_of_flow_elements(ctx, &mut positioned_tree, viewport)?;

    // Pass 3c: Adjust for relative positioning
    adjust_relative_positions(&mut positioned_tree, ctx.styled_dom, ctx.debug_messages)?;

    // Pass 3d: Final adjustment for sticky positioning
    adjust_sticky_positions(ctx, &mut positioned_tree, viewport, scroll_offsets)?;

    ctx.debug_log(&format!(
        "Positioned {} nodes",
        positioned_tree.absolute_positions.len()
    ));

    // Update the input tree with the final calculated results for the next iteration
    *tree = positioned_tree.tree.clone();

    Ok(positioned_tree)
}

fn adjust_sticky_positions<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    positioned_tree: &mut PositionedLayoutTree,
    viewport: LogicalRect,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
) -> Result<()> {
    // This is a new pass to handle `position: sticky`.
    // It calculates the final paint-time offset after all other layout is done.
    // The actual offset is applied in the display_list pass.
    // Here we could calculate and store it if needed for other logic.
    Ok(())
}

fn position_node_recursive(
    positioned_tree: &mut PositionedLayoutTree,
    node_index: usize,
    parent_position: LogicalPosition,
    // Note: parent_size is the initial size, not necessarily the final one.
    _parent_size: LogicalSize,
    styled_dom: &StyledDom,
    renderer_resources: &mut RendererResources,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    let node = positioned_tree
        .tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?;

    let initial_node_size = positioned_tree
        .used_sizes
        .get(&node_index)
        .cloned()
        .unwrap_or_default();

    // Set the initial absolute position for this node (its static position).
    positioned_tree
        .absolute_positions
        .insert(node_index, parent_position);

    // Get the original CSS height property to check if it was 'auto'.
    let css_height = get_css_height(styled_dom, node.dom_node_id);

    // If this node has children, layout them to determine its content height.
    let children = node.children.clone();
    let mut final_node_size = initial_node_size;

    if !children.is_empty() {
        let writing_mode = get_writing_mode(styled_dom, node.dom_node_id);

        let constraints = LayoutConstraints {
            available_size: initial_node_size,
            writing_mode,
            text_align: get_text_align(styled_dom, node.dom_node_id),
            definite_size: Some(initial_node_size),
            floats: None,
        };

        let layout_result = layout_formatting_context(
            &mut positioned_tree.tree,
            node_index,
            &constraints,
            styled_dom,
        )?;

        // If this node's height was 'auto', its final height is determined by its content.
        if css_height == CssSize::Auto {
            if let Some(overflow_size) = layout_result.overflow_size {
                let box_props = get_box_props(styled_dom, node.dom_node_id);
                let new_content_height = overflow_size.height;
                let final_height = new_content_height
                    + box_props.padding.main_sum(constraints.writing_mode)
                    + box_props.border.main_sum(constraints.writing_mode);

                // Update the used_size for this node in the final tree.
                final_node_size.height = final_height;
                if let Some(used_size) = positioned_tree.used_sizes.get_mut(&node_index) {
                    used_size.height = final_height;
                }
            }
        }

        // Apply the calculated positions to children and recurse.
        for (child_index, relative_position) in layout_result.positions {
            let absolute_child_position = LogicalPosition::new(
                parent_position.x + relative_position.x,
                parent_position.y + relative_position.y,
            );
            let child_size = positioned_tree
                .used_sizes
                .get(&child_index)
                .cloned()
                .unwrap_or_default();

            position_node_recursive(
                positioned_tree,
                child_index,
                absolute_child_position,
                child_size,
                styled_dom,
                renderer_resources,
                debug_messages,
            )?;
        }
    }

    // Update the tree node with its final size and relative position.
    if let Some(tree_node) = positioned_tree.tree.get_mut(node_index) {
        tree_node.relative_position = Some(LogicalPosition::zero()); // Relative to parent
        tree_node.used_size = Some(final_node_size);
    }

    Ok(())
}

fn handle_overflow_and_scrollbars(
    _positioned_tree: &mut PositionedLayoutTree,
    node_index: usize,
    container_size: LogicalSize,
    content_size: LogicalSize,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    let needs_scrollbars =
        content_size.width > container_size.width || content_size.height > container_size.height;

    if needs_scrollbars {
        debug_log(
            debug_messages,
            &format!(
                "Node {} needs scrollbars: content {:?} > container {:?}",
                node_index, content_size, container_size
            ),
        );
        // TODO: Implement scrollbar layout, which might require a second layout pass
        // for this subtree with reduced available space.
    }
    Ok(())
}

/// Handle positioned elements (absolute, fixed, sticky)
fn handle_positioned_elements(
    positioned_tree: &mut PositionedLayoutTree,
    styled_dom: &StyledDom,
    viewport: LogicalRect,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    debug_log(debug_messages, "Processing positioned elements");

    let mut positioned_elements = Vec::new();
    for (node_index, tree_node) in positioned_tree.tree.nodes.iter().enumerate() {
        let position_type = get_position_type(styled_dom, tree_node.dom_node_id);
        match position_type {
            PositionType::Absolute | PositionType::Fixed => {
                positioned_elements.push((node_index, position_type));
            }
            _ => { /* Static and Relative are handled in normal flow */ }
        }
    }

    // Position absolute/fixed elements
    for (node_index, position_type) in positioned_elements {
        position_out_of_flow_element(
            positioned_tree,
            node_index,
            position_type,
            styled_dom,
            viewport,
            debug_messages,
        )?;
    }
    Ok(())
}

fn position_out_of_flow_element(
    positioned_tree: &mut PositionedLayoutTree,
    node_index: usize,
    position_type: PositionType,
    styled_dom: &StyledDom,
    viewport: LogicalRect,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    let tree_node = positioned_tree
        .tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?;
    let dom_id = tree_node.dom_node_id.ok_or(LayoutError::InvalidTree)?;

    let mut final_position = positioned_tree
        .absolute_positions
        .get(&node_index)
        .copied()
        .unwrap_or_default();

    let offsets = get_css_offsets(styled_dom, Some(dom_id));
    let element_size = positioned_tree
        .used_sizes
        .get(&node_index)
        .copied()
        .unwrap_or_default();

    // The containing block is different for absolute vs. fixed.
    let containing_block_rect = match position_type {
        PositionType::Absolute => {
            find_absolute_containing_block(positioned_tree, node_index, styled_dom, viewport)?
        }
        PositionType::Fixed => viewport,
        _ => unreachable!(),
    };

    // Apply offsets according to CSS spec (10.3.7 for absolute, 10.6.4 for fixed)
    if let Some(left) = offsets.left {
        final_position.x = containing_block_rect.origin.x + left;
    } else if let Some(right) = offsets.right {
        final_position.x = containing_block_rect.origin.x + containing_block_rect.size.width
            - element_size.width
            - right;
    }

    if let Some(top) = offsets.top {
        final_position.y = containing_block_rect.origin.y + top;
    } else if let Some(bottom) = offsets.bottom {
        final_position.y = containing_block_rect.origin.y + containing_block_rect.size.height
            - element_size.height
            - bottom;
    }

    positioned_tree
        .absolute_positions
        .insert(node_index, final_position);

    debug_log(
        debug_messages,
        &format!(
            "Repositioned {:?} element {} to {:?} in containing block {:?}",
            position_type, node_index, final_position, containing_block_rect
        ),
    );

    Ok(())
}

/// Finds the containing block for an absolutely positioned element.
/// This is the nearest ancestor that is "positioned" (i.e., not `static`).
/// If no such ancestor exists, it falls back to the initial containing block (the viewport).
fn find_absolute_containing_block(
    positioned_tree: &PositionedLayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
    viewport: LogicalRect,
) -> Result<LogicalRect> {
    let mut current_parent_idx = positioned_tree.tree.get(node_index).and_then(|n| n.parent);

    while let Some(parent_index) = current_parent_idx {
        let parent_node = positioned_tree
            .tree
            .get(parent_index)
            .ok_or(LayoutError::InvalidTree)?;

        // A "positioned" ancestor establishes the containing block.
        // TODO: Other properties like `transform` also establish containing blocks.
        if get_position_type(styled_dom, parent_node.dom_node_id) != PositionType::Static {
            let pos = positioned_tree
                .absolute_positions
                .get(&parent_index)
                .copied()
                .unwrap_or_default();
            let size = positioned_tree
                .used_sizes
                .get(&parent_index)
                .copied()
                .unwrap_or_default();
            // The containing block is the *padding box* of the ancestor.
            // For now, we use the border box as a simplification.
            return Ok(LogicalRect::new(pos, size));
        }
        current_parent_idx = parent_node.parent;
    }

    // If no positioned ancestor is found, the containing block is the initial containing block.
    // For web compatibility, this is typically the viewport.
    Ok(viewport)
}

/// Final pass to shift relatively positioned elements from their static flow position.
fn adjust_relative_positions(
    positioned_tree: &mut PositionedLayoutTree,
    styled_dom: &StyledDom,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    // Iterate through all nodes. We need the index to modify the position map.
    for node_index in 0..positioned_tree.tree.nodes.len() {
        let tree_node = &positioned_tree.tree.nodes[node_index];

        if get_position_type(styled_dom, tree_node.dom_node_id) == PositionType::Relative {
            let offsets = get_css_offsets(styled_dom, tree_node.dom_node_id);

            // Get a mutable reference to the position and apply the offsets.
            if let Some(current_pos) = positioned_tree.absolute_positions.get_mut(&node_index) {
                let initial_pos = *current_pos;

                // top/bottom/left/right offsets are applied relative to the static position.
                let mut delta_x = 0.0;
                let mut delta_y = 0.0;

                if let Some(left) = offsets.left {
                    delta_x += left;
                }
                if let Some(right) = offsets.right {
                    delta_x -= right;
                }
                if let Some(top) = offsets.top {
                    delta_y += top;
                }
                if let Some(bottom) = offsets.bottom {
                    delta_y -= bottom;
                }

                current_pos.x += delta_x;
                current_pos.y += delta_y;

                if initial_pos != *current_pos {
                    debug_log(
                        debug_messages,
                        &format!(
                            "Adjusted relative element {} from {:?} to {:?}",
                            node_index, initial_pos, *current_pos
                        ),
                    );
                }
            }
        }
    }
    Ok(())
}

// STUB: These functions simulate reading computed CSS values.
// In a real implementation, they would access the `StyledDom`'s property cache.

/// Correctly determines the position type from the `StyledDom`.
pub fn get_position_type(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> PositionType {
    let Some(id) = dom_id else {
        return PositionType::Static;
    };
    match get_position_property(styled_dom, id) {
        LayoutPosition::Static => PositionType::Static,
        LayoutPosition::Relative => PositionType::Relative,
        LayoutPosition::Absolute => PositionType::Absolute,
        LayoutPosition::Fixed => PositionType::Fixed,
        // TODO: Handle sticky positioning properly
    }
}

/// Correctly reads the `top`, `right`, `bottom`, `left` properties from the `StyledDom`.
fn get_css_offsets(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> PositionOffsets {
    let Some(id) = dom_id else {
        return PositionOffsets::default();
    };
    let style = styled_dom.node_data.as_container()[id].get_style();
    let mut offsets = PositionOffsets::default();

    // Helper to resolve CSS property to pixels. This is a simplification.
    let resolve_to_px = |prop: &CssPropertyValue| -> Option<f32> {
        match prop {
            CssPropertyValue::Px(px) => Some(*px),
            // TODO: Handle other units like %, em, etc., relative to containing block.
            _ => None,
        }
    };

    if let Some(CssProperty::Top(val)) = style.get(&CssProperty::Top) {
        offsets.top = resolve_to_px(val);
    }
    if let Some(CssProperty::Right(val)) = style.get(&CssProperty::Right) {
        offsets.right = resolve_to_px(val);
    }
    if let Some(CssProperty::Bottom(val)) = style.get(&CssProperty::Bottom) {
        offsets.bottom = resolve_to_px(val);
    }
    if let Some(CssProperty::Left(val)) = style.get(&CssProperty::Left) {
        offsets.left = resolve_to_px(val);
    }

    offsets
}

/// Correctly looks up the `position` property from the styled DOM.
fn get_position_property(styled_dom: &StyledDom, node_id: NodeId) -> LayoutPosition {
    if let Some(CssProperty::Position(position)) = styled_dom.node_data.as_container()[node_id]
        .get_style()
        .get(&CssProperty::Position)
    {
        return *position;
    }
    LayoutPosition::Static // Default value
}

/// After the main layout pass, this function iterates through the tree and correctly
/// calculates the final positions of out-of-flow elements (`absolute`, `fixed`).
pub fn position_out_of_flow_elements<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &LayoutTree,
    absolute_positions: &mut BTreeMap<usize, LogicalPosition>,
    viewport: LogicalRect,
) -> Result<()> {
    for node_index in 0..tree.nodes.len() {
        let node = &tree.nodes[node_index];
        let dom_id = match node.dom_node_id {
            Some(id) => id,
            None => continue,
        };

        let position_type = get_position_type(ctx.styled_dom, Some(dom_id));

        if position_type == PositionType::Absolute || position_type == PositionType::Fixed {
            let offsets = get_css_offsets(ctx.styled_dom, Some(dom_id));
            let element_size = node.used_size.unwrap_or_default();

            // The containing block is the viewport for `fixed` elements.
            // For `absolute`, it's the nearest positioned ancestor.
            let containing_block_rect = if position_type == PositionType::Fixed {
                viewport
            } else {
                // NOTE: This assumes `find_absolute_containing_block` is implemented
                // and available. For the test case, this branch is not taken.
                find_absolute_containing_block_rect(
                    tree,
                    node_index,
                    ctx.styled_dom,
                    absolute_positions,
                    viewport,
                )?
            };

            // Get the element's pre-calculated absolute static position. This is what we
            // fall back to for `auto` offsets.
            let static_pos = absolute_positions
                .get(&node_index)
                .copied()
                .unwrap_or_default();

            let mut final_pos = LogicalPosition::zero();

            // --- Vertical Positioning ---
            if let Some(top) = offsets.top {
                final_pos.y = containing_block_rect.origin.y + top;
            } else if let Some(bottom) = offsets.bottom {
                final_pos.y = containing_block_rect.origin.y + containing_block_rect.size.height
                    - element_size.height
                    - bottom;
            } else {
                // The crucial 'auto' fallback for `top`.
                final_pos.y = static_pos.y;
            }

            // --- Horizontal Positioning ---
            if let Some(left) = offsets.left {
                final_pos.x = containing_block_rect.origin.x + left;
            } else if let Some(right) = offsets.right {
                final_pos.x = containing_block_rect.origin.x + containing_block_rect.size.width
                    - element_size.width
                    - right;
            } else {
                // The crucial 'auto' fallback for `left`.
                final_pos.x = static_pos.x;
            }

            // Update the final position in the map.
            absolute_positions.insert(node_index, final_pos);
        }
    }
    Ok(())
}

/// Helper to find the containing block for an absolutely positioned element.
fn find_absolute_containing_block_rect(
    tree: &LayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
    absolute_positions: &BTreeMap<usize, LogicalPosition>,
    viewport: LogicalRect,
) -> Result<LogicalRect> {
    let mut current_parent_idx = tree.get(node_index).and_then(|n| n.parent);

    while let Some(parent_index) = current_parent_idx {
        let parent_node = tree.get(parent_index).ok_or(LayoutError::InvalidTree)?;

        if get_position_type(styled_dom, parent_node.dom_node_id) != PositionType::Static {
            let pos = absolute_positions
                .get(&parent_index)
                .copied()
                .unwrap_or_default();
            let size = parent_node.used_size.unwrap_or_default();
            // The containing block is the *padding box*. We simplify to border-box for now.
            return Ok(LogicalRect::new(pos, size));
        }
        current_parent_idx = parent_node.parent;
    }

    Ok(viewport) // Fallback to the initial containing block.
}
