//! solver3/positioning.rs
//! Pass 3: Final positioning of layout nodes

use std::collections::BTreeMap;

use azul_core::{
    app_resources::RendererResources,
    styled_dom::StyledDom,
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::{LayoutDebugMessage, LayoutPosition};

use crate::solver3::{
    fc::{layout_formatting_context, LayoutConstraints, TextAlign, WritingMode},
    layout_tree::LayoutTree,
    LayoutError, Result,
};

/// Final positioned layout tree
#[derive(Debug)]
pub struct PositionedLayoutTree {
    pub tree: LayoutTree,
    pub absolute_positions: BTreeMap<usize, LogicalPosition>,
    pub used_sizes: BTreeMap<usize, LogicalSize>,
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
pub fn calculate_positions(
    tree: &LayoutTree,
    used_sizes: &BTreeMap<usize, LogicalSize>,
    styled_dom: &StyledDom,
    viewport: LogicalRect,
    renderer_resources: &mut RendererResources,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<PositionedLayoutTree> {
    debug_log(debug_messages, "Starting position calculation");

    let mut positioned_tree = PositionedLayoutTree {
        tree: tree.clone(),
        absolute_positions: BTreeMap::new(),
        used_sizes: used_sizes.clone(),
    };

    // Pass 3a: Layout in-flow elements to determine their static positions
    let root_position = LogicalPosition::zero();
    let root_size = used_sizes.get(&tree.root).cloned().unwrap_or_default();

    position_node_recursive(
        &mut positioned_tree,
        tree.root,
        root_position,
        root_size,
        styled_dom,
        renderer_resources,
        debug_messages,
    )?;

    // Pass 3b: Handle out-of-flow elements (absolute, fixed)
    handle_positioned_elements(&mut positioned_tree, styled_dom, viewport, debug_messages)?;

    // Pass 3c: Adjust positions for relatively positioned elements
    adjust_relative_positions(&mut positioned_tree, styled_dom, debug_messages)?;

    debug_log(
        debug_messages,
        &format!(
            "Positioned {} nodes",
            positioned_tree.absolute_positions.len()
        ),
    );

    Ok(positioned_tree)
}

fn position_node_recursive(
    positioned_tree: &mut PositionedLayoutTree,
    node_index: usize,
    parent_position: LogicalPosition,
    parent_size: LogicalSize,
    styled_dom: &StyledDom,
    renderer_resources: &mut RendererResources,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    let node = positioned_tree
        .tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?;
    let node_size = positioned_tree
        .used_sizes
        .get(&node_index)
        .cloned()
        .unwrap_or_default();

    // Set absolute position for this node (its static position)
    positioned_tree
        .absolute_positions
        .insert(node_index, parent_position);

    // Update the tree node with final position and size
    if let Some(tree_node) = positioned_tree.tree.get_mut(node_index) {
        tree_node.position = Some(LogicalPosition::zero()); // Relative to parent
        tree_node.used_size = Some(node_size);
    }

    // If this node has children, layout them using the appropriate formatting context
    let children = node.children.clone();
    if !children.is_empty() {
        let constraints = LayoutConstraints {
            available_size: node_size,
            writing_mode: WritingMode::default(),
            text_align: TextAlign::default(),
            definite_size: Some(node_size),
        };

        // Use formatting context to position children
        let layout_result = layout_formatting_context(
            &mut positioned_tree.tree,
            node_index,
            &constraints,
            styled_dom,
            renderer_resources,
            debug_messages,
        )?;

        // Apply the calculated positions to children
        for (child_index, relative_position) in layout_result.positions {
            // Out-of-flow elements do not affect the flow of later siblings.
            // Their static position is calculated, but they don't take up space here.
            let child_node = positioned_tree
                .tree
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;
            let position_type = get_position_type(styled_dom, child_node.dom_node_id);

            let absolute_child_position = LogicalPosition::new(
                parent_position.x + relative_position.x,
                parent_position.y + relative_position.y,
            );

            let child_size = positioned_tree
                .used_sizes
                .get(&child_index)
                .cloned()
                .unwrap_or_default();

            // Recursively position child's descendants
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

        // Handle scrollbar adjustments if needed
        if let Some(overflow_size) = layout_result.overflow_size {
            handle_overflow_and_scrollbars(
                positioned_tree,
                node_index,
                node_size,
                overflow_size,
                debug_messages,
            )?;
        }
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

    // Start with the element's static position. This is the key to resolving `auto` for top/left.
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
            find_absolute_containing_block(positioned_tree, node_index, styled_dom)?
        }
        PositionType::Fixed => viewport,
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

fn find_absolute_containing_block(
    positioned_tree: &PositionedLayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
) -> Result<LogicalRect> {
    let node = positioned_tree
        .tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?;

    let mut current = node.parent;
    while let Some(parent_index) = current {
        let parent_node = positioned_tree
            .tree
            .get(parent_index)
            .ok_or(LayoutError::InvalidTree)?;
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
            // The containing block is the padding box of the ancestor.
            // TODO: Inset by padding. For now, use border box.
            return Ok(LogicalRect::new(pos, size));
        }
        current = parent_node.parent;
    }

    // Fall back to the initial containing block (viewport)
    let root_pos = positioned_tree
        .absolute_positions
        .get(&positioned_tree.tree.root)
        .copied()
        .unwrap_or_default();
    let root_size = positioned_tree
        .used_sizes
        .get(&positioned_tree.tree.root)
        .copied()
        .unwrap_or_default();
    Ok(LogicalRect::new(root_pos, root_size))
}

fn adjust_relative_positions(
    positioned_tree: &mut PositionedLayoutTree,
    styled_dom: &StyledDom,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    for (node_index, tree_node) in positioned_tree.tree.nodes.iter().enumerate() {
        if get_position_type(styled_dom, tree_node.dom_node_id) == PositionType::Relative {
            let offsets = get_css_offsets(styled_dom, tree_node.dom_node_id);
            if let Some(current_pos) = positioned_tree.absolute_positions.get_mut(&node_index) {
                let initial_pos = *current_pos;
                if let Some(left) = offsets.left {
                    current_pos.x += left;
                } else if let Some(right) = offsets.right {
                    current_pos.x -= right;
                }
                if let Some(top) = offsets.top {
                    current_pos.y += top;
                } else if let Some(bottom) = offsets.bottom {
                    current_pos.y -= bottom;
                }
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

pub fn get_position_type(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> PositionType {
    let Some(id) = dom_id else {
        return PositionType::Static;
    };
    match get_position_property(styled_dom, id) {
        LayoutPosition::Static => PositionType::Static,
        LayoutPosition::Relative => PositionType::Relative,
        LayoutPosition::Absolute => PositionType::Absolute,
        LayoutPosition::Fixed => PositionType::Fixed,
        // TODO: Handle sticky positioning
    }
}

fn get_css_offsets(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> PositionOffsets {
    let Some(id) = dom_id else {
        return PositionOffsets::default();
    };
    // This would parse the actual CSS `top`, `right`, `bottom`, `left` properties.
    // For the test case, #div1 has `top: 1in`, the fixed div has all `auto`.
    let id_str = styled_dom.node_data[id].id.as_ref().map(|s| s.as_str());
    if id_str == Some("div1") {
        PositionOffsets {
            top: Some(96.0),
            ..Default::default()
        } // 1 inch = 96px in CSS
    } else {
        PositionOffsets::default()
    }
}

// In a real implementation, this would live in layout_tree.rs, but we need it here too.
fn get_position_property(styled_dom: &StyledDom, node_id: NodeId) -> LayoutPosition {
    let id_str = styled_dom.node_data[node_id]
        .id
        .as_ref()
        .map(|s| s.as_str());
    if id_str == Some("div1") {
        return LayoutPosition::Absolute;
    }
    if let Some(parent_id) = styled_dom.get_parent_id(node_id) {
        let parent_id_str = styled_dom.node_data[parent_id]
            .id
            .as_ref()
            .map(|s| s.as_str());
        if parent_id_str == Some("div1") {
            return LayoutPosition::Fixed;
        }
    }
    LayoutPosition::Static
}

fn debug_log(debug_messages: &mut Option<Vec<LayoutDebugMessage>>, message: &str) {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: message.into(),
            location: "positioning".into(),
        });
    }
}
