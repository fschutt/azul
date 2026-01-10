//! solver3/positioning.rs
//! Pass 3: Final positioning of layout nodes

use std::collections::BTreeMap;

use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{
    corety::LayoutDebugMessage,
    css::CssPropertyValue,
    props::{
        basic::pixel::PixelValue,
        layout::{LayoutPosition, LayoutWritingMode},
        property::{CssProperty, CssPropertyType},
    },
};

use crate::{
    font_traits::{FontLoaderTrait, ParsedFontTrait},
    solver3::{
        fc::{layout_formatting_context, LayoutConstraints, TextAlign},
        getters::get_writing_mode,
        layout_tree::LayoutTree,
        LayoutContext, LayoutError, Result,
    },
};

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
pub fn get_position_type(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutPosition {
    let Some(id) = dom_id else {
        return LayoutPosition::Static;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_position(node_data, &id, node_state)
        .and_then(|w| w.get_property().cloned())
        .unwrap_or_default()
}

/// Correctly looks up the `position` property from the styled DOM.
fn get_position_property(styled_dom: &StyledDom, node_id: NodeId) -> LayoutPosition {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_position(node_data, &node_id, node_state)
        .and_then(|p| p.get_property().copied())
        .unwrap_or(LayoutPosition::Static)
}

/// **NEW API:** Correctly reads and resolves `top`, `right`, `bottom`, `left` properties,
/// including percentages relative to the containing block's size, and em/rem units.
/// Uses the modern resolve_with_context() API.
fn resolve_position_offsets(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
    cb_size: LogicalSize,
) -> PositionOffsets {
    use azul_css::props::basic::pixel::{PhysicalSize, PropertyContext, ResolutionContext};

    use crate::solver3::getters::{
        get_element_font_size, get_parent_font_size, get_root_font_size,
    };

    let Some(id) = dom_id else {
        return PositionOffsets::default();
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;

    // Create resolution context with font sizes and containing block size
    let element_font_size = get_element_font_size(styled_dom, id, node_state);
    let parent_font_size = get_parent_font_size(styled_dom, id, node_state);
    let root_font_size = get_root_font_size(styled_dom, node_state);

    let containing_block_size = PhysicalSize::new(cb_size.width, cb_size.height);

    let resolution_context = ResolutionContext {
        element_font_size,
        parent_font_size,
        root_font_size,
        containing_block_size,
        element_size: None, // Not needed for position offsets
        viewport_size: PhysicalSize::new(0.0, 0.0),
    };

    let mut offsets = PositionOffsets::default();

    // Resolve offsets with proper context
    // top/bottom use Height context (% refers to containing block height)
    offsets.top = styled_dom
        .css_property_cache
        .ptr
        .get_top(node_data, &id, node_state)
        .and_then(|t| t.get_property())
        .map(|v| {
            v.inner
                .resolve_with_context(&resolution_context, PropertyContext::Height)
        });

    offsets.bottom = styled_dom
        .css_property_cache
        .ptr
        .get_bottom(node_data, &id, node_state)
        .and_then(|b| b.get_property())
        .map(|v| {
            v.inner
                .resolve_with_context(&resolution_context, PropertyContext::Height)
        });

    // left/right use Width context (% refers to containing block width)
    offsets.left = styled_dom
        .css_property_cache
        .ptr
        .get_left(node_data, &id, node_state)
        .and_then(|l| l.get_property())
        .map(|v| {
            v.inner
                .resolve_with_context(&resolution_context, PropertyContext::Width)
        });

    offsets.right = styled_dom
        .css_property_cache
        .ptr
        .get_right(node_data, &id, node_state)
        .and_then(|r| r.get_property())
        .map(|v| {
            v.inner
                .resolve_with_context(&resolution_context, PropertyContext::Width)
        });

    offsets
}

/// After the main layout pass, this function iterates through the tree and correctly
/// calculates the final positions of out-of-flow elements (`absolute`, `fixed`).
pub fn position_out_of_flow_elements<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
    viewport: LogicalRect,
) -> Result<()> {
    for node_index in 0..tree.nodes.len() {
        let node = &tree.nodes[node_index];
        let dom_id = match node.dom_node_id {
            Some(id) => id,
            None => continue,
        };

        let position_type = get_position_type(ctx.styled_dom, Some(dom_id));

        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            // Get parent info before any mutable borrows
            let parent_info: Option<(usize, LogicalPosition, f32, f32, f32, f32)> = {
                let node = &tree.nodes[node_index];
                node.parent.and_then(|parent_idx| {
                    let parent_node = tree.get(parent_idx)?;
                    let parent_dom_id = parent_node.dom_node_id?;
                    let parent_position = get_position_type(ctx.styled_dom, Some(parent_dom_id));
                    if parent_position == LayoutPosition::Absolute
                        || parent_position == LayoutPosition::Fixed
                    {
                        calculated_positions.get(&parent_idx).map(|parent_pos| {
                            (
                                parent_idx,
                                *parent_pos,
                                parent_node.box_props.border.left,
                                parent_node.box_props.border.top,
                                parent_node.box_props.padding.left,
                                parent_node.box_props.padding.top,
                            )
                        })
                    } else {
                        None
                    }
                })
            };

            // Determine containing block FIRST (before calculating size)
            let containing_block_rect = if position_type == LayoutPosition::Fixed {
                viewport
            } else {
                find_absolute_containing_block_rect(
                    tree,
                    node_index,
                    ctx.styled_dom,
                    calculated_positions,
                    viewport,
                )?
            };

            // Get node again after containing block calculation
            let node = &tree.nodes[node_index];

            // Calculate used size for out-of-flow elements (they don't get sized during normal
            // layout)
            let element_size = if let Some(size) = node.used_size {
                size
            } else {
                // Element hasn't been sized yet - calculate it now using containing block
                let intrinsic = node.intrinsic_sizes.unwrap_or_default();
                let size = crate::solver3::sizing::calculate_used_size_for_node(
                    ctx.styled_dom,
                    Some(dom_id),
                    containing_block_rect.size,
                    intrinsic,
                    &node.box_props,
                )?;

                // Store the calculated size in the tree node
                if let Some(node_mut) = tree.get_mut(node_index) {
                    node_mut.used_size = Some(size);
                }

                size
            };

            // Resolve offsets using the now-known containing block size.
            let offsets =
                resolve_position_offsets(ctx.styled_dom, Some(dom_id), containing_block_rect.size);

            let mut static_pos = calculated_positions
                .get(&node_index)
                .copied()
                .unwrap_or_default();

            // Special case: If this is a fixed-position element with (0,0) static position
            // and it has a positioned parent, use the parent's content-box position
            if position_type == LayoutPosition::Fixed && static_pos == LogicalPosition::zero() {
                if let Some((_, parent_pos, border_left, border_top, padding_left, padding_top)) =
                    parent_info
                {
                    // Add parent's border and padding to get content-box position
                    static_pos = LogicalPosition::new(
                        parent_pos.x + border_left + padding_left,
                        parent_pos.y + border_top + padding_top,
                    );
                }
            }

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

            calculated_positions.insert(node_index, final_pos);
        }
    }
    Ok(())
}

/// Final pass to shift relatively positioned elements from their static flow position.
///
/// This function now correctly resolves percentage-based offsets for `top`, `left`, etc.
/// According to the CSS spec, for relatively positioned elements, these percentages are
/// relative to the dimensions of the parent element's content box.
pub fn adjust_relative_positions<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
    viewport: LogicalRect, // The viewport is needed if the root element is relative.
) -> Result<()> {
    // Iterate through all nodes. We need the index to modify the position map.
    for node_index in 0..tree.nodes.len() {
        let node = &tree.nodes[node_index];
        let position_type = get_position_type(ctx.styled_dom, node.dom_node_id);

        // Early continue for non-relative positioning
        if position_type != LayoutPosition::Relative {
            continue;
        }

        // Determine the containing block size for resolving percentages.
        // For `position: relative`, this is the parent's content box size.
        let containing_block_size = node.parent
            .and_then(|parent_idx| tree.get(parent_idx))
            .map(|parent_node| {
                // Get parent's writing mode to correctly calculate its inner (content) size.
                let parent_dom_id = parent_node.dom_node_id.unwrap_or(NodeId::ZERO);
                let parent_node_state =
                    &ctx.styled_dom.styled_nodes.as_container()[parent_dom_id].styled_node_state;
                let parent_wm =
                    get_writing_mode(ctx.styled_dom, parent_dom_id, parent_node_state)
                    .unwrap_or_default();
                let parent_used_size = parent_node.used_size.unwrap_or_default();
                parent_node.box_props.inner_size(parent_used_size, parent_wm)
            })
            // The root element is relatively positioned. Its containing block is the viewport.
            .unwrap_or(viewport.size);

        // Resolve offsets using the calculated containing block size.
        let offsets =
            resolve_position_offsets(ctx.styled_dom, node.dom_node_id, containing_block_size);

        // Get a mutable reference to the position and apply the offsets.
        let Some(current_pos) = calculated_positions.get_mut(&node_index) else {
            continue;
        };

        let initial_pos = *current_pos;

        // top/bottom/left/right offsets are applied relative to the static position.
        let mut delta_x = 0.0;
        let mut delta_y = 0.0;

        // According to CSS 2.1 Section 9.3.2:
        // - For `top` and `bottom`: if both are specified, `top` wins and `bottom` is ignored
        // - For `left` and `right`: depends on direction (ltr/rtl)
        //   - In LTR: if both specified, `left` wins and `right` is ignored
        //   - In RTL: if both specified, `right` wins and `left` is ignored

        // Vertical positioning: `top` takes precedence over `bottom`
        if let Some(top) = offsets.top {
            delta_y = top;
        } else if let Some(bottom) = offsets.bottom {
            delta_y = -bottom;
        }

        // Horizontal positioning: depends on direction
        // Get the direction for this element
        let node_dom_id = node.dom_node_id.unwrap_or(NodeId::ZERO);
        let node_data = &ctx.styled_dom.node_data.as_container()[node_dom_id];
        let node_state = &ctx.styled_dom.styled_nodes.as_container()[node_dom_id].styled_node_state;
        let direction = ctx
            .styled_dom
            .css_property_cache
            .ptr
            .get_direction(node_data, &node_dom_id, node_state)
            .and_then(|s| s.get_property().copied())
            .unwrap_or(azul_css::props::style::StyleDirection::Ltr);

        use azul_css::props::style::StyleDirection;
        match direction {
            StyleDirection::Ltr => {
                // In LTR mode: `left` takes precedence over `right`
                if let Some(left) = offsets.left {
                    delta_x = left;
                } else if let Some(right) = offsets.right {
                    delta_x = -right;
                }
            }
            StyleDirection::Rtl => {
                // In RTL mode: `right` takes precedence over `left`
                if let Some(right) = offsets.right {
                    delta_x = -right;
                } else if let Some(left) = offsets.left {
                    delta_x = left;
                }
            }
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
    Ok(())
}

/// Helper to find the containing block for an absolutely positioned element.
/// CSS 2.1 Section 10.1: The containing block for absolutely positioned elements
/// is the padding box of the nearest positioned ancestor.
fn find_absolute_containing_block_rect(
    tree: &LayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
    calculated_positions: &BTreeMap<usize, LogicalPosition>,
    viewport: LogicalRect,
) -> Result<LogicalRect> {
    let mut current_parent_idx = tree.get(node_index).and_then(|n| n.parent);

    while let Some(parent_index) = current_parent_idx {
        let parent_node = tree.get(parent_index).ok_or(LayoutError::InvalidTree)?;

        if get_position_type(styled_dom, parent_node.dom_node_id) != LayoutPosition::Static {
            // calculated_positions stores margin-box positions
            let margin_box_pos = calculated_positions
                .get(&parent_index)
                .copied()
                .unwrap_or_default();
            // used_size is the border-box size
            let border_box_size = parent_node.used_size.unwrap_or_default();

            // Calculate padding-box origin (margin-box + border)
            // CSS 2.1 § 10.1: containing block is the padding box
            let padding_box_pos = LogicalPosition::new(
                margin_box_pos.x + parent_node.box_props.border.left,
                margin_box_pos.y + parent_node.box_props.border.top,
            );

            // Calculate padding-box size (border-box - borders)
            let padding_box_size = LogicalSize::new(
                border_box_size.width
                    - parent_node.box_props.border.left
                    - parent_node.box_props.border.right,
                border_box_size.height
                    - parent_node.box_props.border.top
                    - parent_node.box_props.border.bottom,
            );

            return Ok(LogicalRect::new(padding_box_pos, padding_box_size));
        }
        current_parent_idx = parent_node.parent;
    }

    Ok(viewport) // Fallback to the initial containing block.
}
