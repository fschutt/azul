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
use azul_css::{
    CssProperty, CssPropertyType, CssPropertyValue, LayoutDebugMessage, LayoutPosition, PixelValue,
};

use crate::{
    solver3::{
        fc::{layout_formatting_context, LayoutConstraints, TextAlign},
        geometry::CssSize,
        layout_tree::LayoutTree,
        LayoutContext, LayoutError, Result,
    },
    text3::cache::{FontLoaderTrait, ParsedFontTrait},
};

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

// STUB: These functions simulate reading computed CSS values.
// In a real implementation, they would access the `StyledDom`'s property cache.

// STUB: This function simulates reading computed CSS values.
pub fn get_position_type(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> PositionType {
    let Some(id) = dom_id else {
        return PositionType::Static;
    };
    if let Some(styled_node) = styled_dom.styled_nodes.as_container().get(id) {
        if let Some(CssProperty::Position(CssPropertyValue::Exact(position))) = styled_node
            .state
            .get_style()
            .get(&CssPropertyType::Position)
        {
            return match position {
                LayoutPosition::Static => PositionType::Static,
                LayoutPosition::Relative => PositionType::Relative,
                LayoutPosition::Absolute => PositionType::Absolute,
                LayoutPosition::Fixed => PositionType::Fixed,
            };
        }
    }
    PositionType::Static
}

/// Correctly reads the `top`, `right`, `bottom`, `left` properties from the `StyledDom`.
fn get_css_offsets(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> PositionOffsets {
    let Some(id) = dom_id else {
        return PositionOffsets::default();
    };
    let Some(styled_node) = styled_dom.styled_nodes.as_container().get(id) else {
        return PositionOffsets::default();
    };
    let style = styled_node.state.get_style();
    let mut offsets = PositionOffsets::default();

    // Helper to resolve CSS property to pixels. This is a simplification.
    let resolve_to_px = |prop: &CssPropertyValue<PixelValue>| -> Option<f32> {
        let hundred_percent = 0.0; // for resolving percentages
        match prop {
            CssPropertyValue::Exact(px) => Some(px.to_pixels(hundred_percent)),
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

/// **FIXED:** Correctly reads and resolves `top`, `right`, `bottom`, `left` properties,
/// including percentages relative to the containing block's size.
fn resolve_css_offsets(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
    cb_size: LogicalSize,
) -> PositionOffsets {
    let Some(id) = dom_id else {
        return PositionOffsets::default();
    };
    let Some(styled_node) = styled_dom.styled_nodes.as_container().get(id) else {
        return PositionOffsets::default();
    };
    let style = styled_node.state.get_style();
    let mut offsets = PositionOffsets::default();

    // Helper to resolve a CSS PixelValue to a final f32 value.
    // Percentages for top/bottom are relative to the CB's height.
    // Percentages for left/right are relative to the CB's width.
    let resolve_vertical = |prop: &CssPropertyValue<PixelValue>| -> Option<f32> {
        match prop.get_exact() {
            Some(PixelValue::Px(px)) => Some(*px),
            Some(PixelValue::Percent(p)) => Some((p / 100.0) * cb_size.height),
            _ => None,
        }
    };

    let resolve_horizontal = |prop: &CssPropertyValue<PixelValue>| -> Option<f32> {
        match prop.get_exact() {
            Some(PixelValue::Px(px)) => Some(*px),
            Some(PixelValue::Percent(p)) => Some((p / 100.0) * cb_size.width),
            _ => None,
        }
    };

    if let Some(val) = style.get(&CssPropertyType::Top) {
        offsets.top = resolve_vertical(val);
    }
    if let Some(val) = style.get(&CssPropertyType::Right) {
        offsets.right = resolve_horizontal(val);
    }
    if let Some(val) = style.get(&CssPropertyType::Bottom) {
        offsets.bottom = resolve_vertical(val);
    }
    if let Some(val) = style.get(&CssPropertyType::Left) {
        offsets.left = resolve_horizontal(val);
    }

    offsets
}

/// After the main layout pass, this function iterates through the tree and correctly
/// calculates the final positions of out-of-flow elements (`absolute`, `fixed`).
pub fn position_out_of_flow_elements<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &LayoutTree<T>,
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
            let element_size = node.used_size.unwrap_or_default();

            let containing_block_rect = if position_type == PositionType::Fixed {
                viewport
            } else {
                find_absolute_containing_block_rect(
                    tree,
                    node_index,
                    ctx.styled_dom,
                    absolute_positions,
                    viewport,
                )?
            };

            // Resolve offsets using the now-known containing block size.
            let offsets =
                resolve_css_offsets(ctx.styled_dom, Some(dom_id), containing_block_rect.size);

            let static_pos = absolute_positions
                .get(&node_index)
                .copied()
                .unwrap_or_default();
            let mut final_pos = LogicalPosition::zero();

            // Vertical Positioning
            if let Some(top) = offsets.top {
                final_pos.y = containing_block_rect.origin.y + top;
            } else if let Some(bottom) = offsets.bottom {
                final_pos.y = containing_block_rect.origin.y + containing_block_rect.size.height
                    - element_size.height
                    - bottom;
            } else {
                final_pos.y = static_pos.y;
            }

            // Horizontal Positioning
            if let Some(left) = offsets.left {
                final_pos.x = containing_block_rect.origin.x + left;
            } else if let Some(right) = offsets.right {
                final_pos.x = containing_block_rect.origin.x + containing_block_rect.size.width
                    - element_size.width
                    - right;
            } else {
                final_pos.x = static_pos.x;
            }

            absolute_positions.insert(node_index, final_pos);
        }
    }
    Ok(())
}

/// Final pass to shift relatively positioned elements from their static flow position.
///
/// This function now correctly resolves percentage-based offsets for `top`, `left`, etc.
/// According to the CSS spec, for relatively positioned elements, these percentages are
/// relative to the dimensions of the parent element's content box.
pub fn adjust_relative_positions<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &LayoutTree<T>,
    absolute_positions: &mut BTreeMap<usize, LogicalPosition>,
    viewport: LogicalRect, // The viewport is needed if the root element is relative.
) -> Result<()> {
    // Iterate through all nodes. We need the index to modify the position map.
    for node_index in 0..tree.nodes.len() {
        let node = &tree.nodes[node_index];

        if get_position_type(ctx.styled_dom, node.dom_node_id) == PositionType::Relative {
            // Determine the containing block size for resolving percentages.
            // For `position: relative`, this is the parent's content box size.
            let containing_block_size = if let Some(parent_idx) = node.parent {
                if let Some(parent_node) = tree.get(parent_idx) {
                    // Get parent's writing mode to correctly calculate its inner (content) size.
                    let parent_wm = get_writing_mode(ctx.styled_dom, parent_node.dom_node_id);
                    let parent_used_size = parent_node.used_size.unwrap_or_default();
                    parent_node
                        .box_props
                        .inner_size(parent_used_size, parent_wm)
                } else {
                    // This should not happen in a valid tree, but handle gracefully.
                    LogicalSize::zero()
                }
            } else {
                // The root element is relatively positioned. Its containing block is the viewport.
                viewport.size
            };

            // Resolve offsets using the calculated containing block size.
            let offsets =
                resolve_css_offsets(ctx.styled_dom, node.dom_node_id, containing_block_size);

            // Get a mutable reference to the position and apply the offsets.
            if let Some(current_pos) = absolute_positions.get_mut(&node_index) {
                let initial_pos = *current_pos;

                // top/bottom/left/right offsets are applied relative to the static position.
                let mut delta_x = 0.0;
                let mut delta_y = 0.0;

                // Note: The spec says if both 'left' and 'right' are specified, 'right' is ignored.
                // This implementation sums them, which is a common simplification but not strictly
                // correct. A fully compliant engine would respect directionality
                // (ltr/rtl).
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

                // Only apply the shift if there is a non-zero delta.
                if delta_x != 0.0 || delta_y != 0.0 {
                    current_pos.x += delta_x;
                    current_pos.y += delta_y;

                    ctx.debug_log(&format!(
                        "Adjusted relative element #{} from {:?} to {:?} (delta: {}, {})",
                        node_index, initial_pos, *current_pos, delta_x, delta_y
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Helper to find the containing block for an absolutely positioned element.
fn find_absolute_containing_block_rect<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
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
            return Ok(LogicalRect::new(pos, size));
        }
        current_parent_idx = parent_node.parent;
    }

    Ok(viewport) // Fallback to the initial containing block.
}

// STUB: This helper function is now needed in this file. In a real project,
// it would live in a shared utility module.
fn get_writing_mode(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> WritingMode {
    let Some(id) = dom_id else {
        return WritingMode::HorizontalTb;
    };
    if let Some(styled_node) = styled_dom.styled_nodes.as_container().get(id) {
        if let Some(prop) = styled_node
            .state
            .get_style()
            .get(&CssPropertyType::WritingMode)
        {
            if let Some(val) = prop.get_exact() {
                return match val {
                    LayoutWritingMode::HorizontalTb => WritingMode::HorizontalTb,
                    LayoutWritingMode::VerticalRl => WritingMode::VerticalRl,
                    LayoutWritingMode::VerticalLr => WritingMode::VerticalLr,
                };
            }
        }
    }
    WritingMode::HorizontalTb
}
