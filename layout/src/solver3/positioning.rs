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
        getters::{
            get_direction_property, get_writing_mode, get_position, MultiValue,
            get_css_top, get_css_bottom, get_css_left, get_css_right,
            get_css_height, get_css_width,
        },
        layout_tree::LayoutTree,
        LayoutContext, LayoutError, Result,
    },
};

#[derive(Debug, Default)]
pub struct PositionOffsets {
    top: Option<f32>,
    right: Option<f32>,
    bottom: Option<f32>,
    left: Option<f32>,
}

/// Looks up the `position` property using the compact-cache-aware getter.
pub fn get_position_type(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutPosition {
    let Some(id) = dom_id else {
        return LayoutPosition::Static;
    };
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    get_position(styled_dom, id, node_state).unwrap_or_default()
}

/// Correctly looks up the `position` property from the styled DOM.
fn get_position_property(styled_dom: &StyledDom, node_id: NodeId) -> LayoutPosition {
    let node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    get_position(styled_dom, node_id, node_state).unwrap_or(LayoutPosition::Static)
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

    // Resolve offsets using compact-cache-aware getters
    // top/bottom use Height context (% refers to containing block height)
    offsets.top = match get_css_top(styled_dom, id, node_state) {
        MultiValue::Exact(pv) => Some(pv.resolve_with_context(&resolution_context, PropertyContext::Height)),
        _ => None,
    };

    offsets.bottom = match get_css_bottom(styled_dom, id, node_state) {
        MultiValue::Exact(pv) => Some(pv.resolve_with_context(&resolution_context, PropertyContext::Height)),
        _ => None,
    };

    // left/right use Width context (% refers to containing block width)
    offsets.left = match get_css_left(styled_dom, id, node_state) {
        MultiValue::Exact(pv) => Some(pv.resolve_with_context(&resolution_context, PropertyContext::Width)),
        _ => None,
    };

    offsets.right = match get_css_right(styled_dom, id, node_state) {
        MultiValue::Exact(pv) => Some(pv.resolve_with_context(&resolution_context, PropertyContext::Width)),
        _ => None,
    };

    offsets
}

/// After the main layout pass, this function iterates through the tree and correctly
/// calculates the final positions of out-of-flow elements (`absolute`, `fixed`).
pub fn position_out_of_flow_elements<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    calculated_positions: &mut super::PositionVec,
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
                        calculated_positions.get(parent_idx).map(|parent_pos| {
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

            // +spec:containing-block-p001 +spec:containing-block-p032 - fixed: CB is viewport; absolute: CB is padding edge of nearest positioned ancestor
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
                // +spec:height-calculation-p009 - §10.6.4: abs-pos CB height is always definite (independent of element content)
                let size = crate::solver3::sizing::calculate_used_size_for_node(
                    ctx.styled_dom,
                    Some(dom_id),
                    containing_block_rect.size,
                    intrinsic,
                    &node.box_props,
                    ctx.viewport_size,
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
                .get(node_index)
                .copied()
                .unwrap_or_default();

            // Special case: If this is a fixed-position element and it has a positioned
            // parent, update static_pos to be relative to the parent's final absolute
            // position (content-box). The initial static_pos from process_out_of_flow_children
            // may include border/padding offsets, so we must always recalculate here.
            if position_type == LayoutPosition::Fixed {
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

            // +spec:containing-block-p016 - top/left offsets position relative to containing block
            // +spec:containing-block-p028 - §10.6.4: constraint equation for abspos vertical dimensions
            // top + margin-top + border-top + padding-top + height + padding-bottom +
            // border-bottom + margin-bottom + bottom = containing block height
            let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

            // Extract all box_props values upfront to avoid borrow conflicts with tree.get_mut()
            let (margin_top_val, margin_bottom_val, margin_auto,
                 margin_left_val, margin_right_val, margin_left_auto_flag, margin_right_auto_flag) = {
                let node = &tree.nodes[node_index];
                (node.box_props.margin.top, node.box_props.margin.bottom,
                 node.box_props.margin_auto,
                 node.box_props.margin.left, node.box_props.margin.right,
                 node.box_props.margin_auto.left, node.box_props.margin_auto.right)
            };
            let cb_height = containing_block_rect.size.height;

            let css_height = get_css_height(ctx.styled_dom, dom_id, node_state);
            let height_is_auto = css_height.is_auto();
            let top_is_auto = offsets.top.is_none();
            let bottom_is_auto = offsets.bottom.is_none();

            // element_size is border-box (includes border + padding + content).
            // The constraint equation is:
            //   top + margin-top + border-box-height + margin-bottom + bottom = CB height
            // (border-top, padding-top, content-height, padding-bottom, border-bottom
            //  are all inside border-box-height)
            let mut used_height = element_size.height;
            let mut used_margin_top = if margin_auto.top { 0.0 } else { margin_top_val };
            let mut used_margin_bottom = if margin_auto.bottom { 0.0 } else { margin_bottom_val };

            if top_is_auto && height_is_auto && bottom_is_auto {
                // All three auto: set top to static position, height from content, solve for bottom
                final_pos.y = static_pos.y;
            } else if !top_is_auto && !height_is_auto && !bottom_is_auto {
                // None are auto: over-constrained case
                let top_val = offsets.top.unwrap();
                let bottom_val = offsets.bottom.unwrap();
                if margin_auto.top && margin_auto.bottom {
                    let available = cb_height - top_val - used_height - bottom_val;
                    let each = available / 2.0;
                    used_margin_top = each;
                    used_margin_bottom = each;
                } else if margin_auto.top {
                    used_margin_top = cb_height - top_val - used_height - used_margin_bottom - bottom_val;
                } else if margin_auto.bottom {
                    used_margin_bottom = cb_height - top_val - used_height - used_margin_top - bottom_val;
                }
                // else: over-constrained, ignore bottom
                final_pos.y = containing_block_rect.origin.y + top_val + used_margin_top;
            } else if top_is_auto && height_is_auto && !bottom_is_auto {
                // Rule 1: height from content, auto margins to 0, solve for top
                let bottom_val = offsets.bottom.unwrap();
                let top_val = cb_height - used_margin_top - used_height - used_margin_bottom - bottom_val;
                final_pos.y = containing_block_rect.origin.y + top_val + used_margin_top;
            } else if top_is_auto && bottom_is_auto && !height_is_auto {
                // Rule 2: set top to static position, auto margins to 0, solve for bottom
                final_pos.y = static_pos.y;
            } else if height_is_auto && bottom_is_auto && !top_is_auto {
                // Rule 3: height from content, auto margins to 0, solve for bottom
                let top_val = offsets.top.unwrap();
                final_pos.y = containing_block_rect.origin.y + top_val + used_margin_top;
            } else if top_is_auto && !height_is_auto && !bottom_is_auto {
                // Rule 4: auto margins to 0, solve for top
                let bottom_val = offsets.bottom.unwrap();
                let top_val = cb_height - used_margin_top - used_height - used_margin_bottom - bottom_val;
                final_pos.y = containing_block_rect.origin.y + top_val + used_margin_top;
            } else if height_is_auto && !top_is_auto && !bottom_is_auto {
                // +spec:height-calculation-p016 - §10.6.4 rule 5: height auto, top and bottom not auto
                // solve for height from constraint equation:
                // height = cb_height - top - margin_top - margin_bottom - bottom
                let top_val = offsets.top.unwrap();
                let bottom_val = offsets.bottom.unwrap();
                used_height = (cb_height - top_val - used_margin_top - used_margin_bottom - bottom_val).max(0.0);
                final_pos.y = containing_block_rect.origin.y + top_val + used_margin_top;
                // Update the element size with the resolved height
                if let Some(node_mut) = tree.get_mut(node_index) {
                    if let Some(ref mut size) = node_mut.used_size {
                        size.height = used_height;
                    }
                }
            } else if bottom_is_auto && !top_is_auto && !height_is_auto {
                // Rule 6: auto margins to 0, solve for bottom
                let top_val = offsets.top.unwrap();
                final_pos.y = containing_block_rect.origin.y + top_val + used_margin_top;
            } else {
                // Fallback to static position
                final_pos.y = static_pos.y;
            }

            // +spec:width-calculation-p036 - §10.3.7: horizontal constraint for abspos non-replaced elements
            // +spec:width-calculation-p002 - §10.3.7: horizontal constraint equation for absolutely positioned, non-replaced elements
            // +spec:width-calculation-p001 - §10.3.7/§10.3.8: horizontal constraint for abs-pos elements
            // +spec:width-calculation-p010 - §10.3.8: abs-pos replaced elements use inline replaced width; auto margins resolved by horizontal constraint
            // +spec:width-calculation-p032 - §10.3.8: abs-pos replaced elements with both margins auto, solve with equal values
            // +spec:width-calculation-p050 - §10.3.7: horizontal constraint for absolutely positioned, non-replaced elements
            // Constraint: left + margin-left + border-left + padding-left + width +
            //   padding-right + border-right + margin-right + right = CB width
            // Since element_size.width is border-box (border + padding + content),
            // simplifies to: left + margin-left + border_box_width + margin-right + right = CB width
            {
                let margin_left = margin_left_val;
                let margin_right = margin_right_val;
                let margin_left_auto = margin_left_auto_flag;
                let margin_right_auto = margin_right_auto_flag;
                let cb_width = containing_block_rect.size.width;
                let border_box_width = element_size.width;
                let left_val = offsets.left;
                let right_val = offsets.right;
                let left_is_auto = left_val.is_none();
                let right_is_auto = right_val.is_none();

                // Get direction of containing block for over-constrained resolution
                use azul_css::props::style::StyleDirection;
                let cb_direction = {
                    let cb_dom_id = if position_type == LayoutPosition::Fixed {
                        None // viewport CB, default LTR
                    } else {
                        let mut parent = tree.nodes[node_index].parent;
                        let mut found = None;
                        while let Some(pidx) = parent {
                            if let Some(pnode) = tree.get(pidx) {
                                if get_position_type(ctx.styled_dom, pnode.dom_node_id) != LayoutPosition::Static {
                                    found = pnode.dom_node_id;
                                    break;
                                }
                                parent = pnode.parent;
                            } else {
                                break;
                            }
                        }
                        found
                    };
                    match cb_dom_id {
                        Some(cb_id) => {
                            let cb_ns = &ctx.styled_dom.styled_nodes.as_container()[cb_id].styled_node_state;
                            match get_direction_property(ctx.styled_dom, cb_id, cb_ns) {
                                MultiValue::Exact(v) => v,
                                _ => StyleDirection::Ltr,
                            }
                        }
                        None => StyleDirection::Ltr,
                    }
                };

                let width_is_auto = get_css_width(ctx.styled_dom, dom_id, node_state).is_auto();

                if !left_is_auto && !width_is_auto && !right_is_auto {
                    // None of left/width/right are auto — solve for margins or handle over-constrained
                    let left = left_val.unwrap();
                    let right = right_val.unwrap();
                    let remaining = cb_width - left - border_box_width - right;

                    if margin_left_auto && margin_right_auto {
                        // Both margins auto: equal values unless negative
                        let each_margin = remaining / 2.0;
                        if each_margin < 0.0 {
                            match cb_direction {
                                StyleDirection::Ltr => {
                                    final_pos.x = containing_block_rect.origin.x + left;
                                }
                                StyleDirection::Rtl => {
                                    final_pos.x = containing_block_rect.origin.x + left + remaining;
                                }
                            }
                        } else {
                            final_pos.x = containing_block_rect.origin.x + left + each_margin;
                        }
                    } else if margin_left_auto {
                        let solved_margin_left = remaining - margin_right;
                        final_pos.x = containing_block_rect.origin.x + left + solved_margin_left;
                    } else if margin_right_auto {
                        final_pos.x = containing_block_rect.origin.x + left + margin_left;
                    } else {
                        // Over-constrained: ignore right (LTR) or left (RTL)
                        match cb_direction {
                            StyleDirection::Ltr => {
                                final_pos.x = containing_block_rect.origin.x + left + margin_left;
                            }
                            StyleDirection::Rtl => {
                                let solved_left = cb_width - margin_left - border_box_width - margin_right - right;
                                final_pos.x = containing_block_rect.origin.x + solved_left + margin_left;
                            }
                        }
                    }
                } else {
                    // Set auto margins to 0, apply six rules
                    let m_left = if margin_left_auto { 0.0 } else { margin_left };
                    let m_right = if margin_right_auto { 0.0 } else { margin_right };

                    if left_is_auto && width_is_auto && right_is_auto {
                        // All three auto: use static position
                        final_pos.x = static_pos.x;
                    } else if left_is_auto && width_is_auto && !right_is_auto {
                        // left+width auto, right not auto: width from content, solve for left
                        let right = right_val.unwrap();
                        let solved_left = cb_width - m_left - border_box_width - m_right - right;
                        final_pos.x = containing_block_rect.origin.x + solved_left + m_left;
                    } else if left_is_auto && !width_is_auto && right_is_auto {
                        // left+right auto: set left to static position (LTR)
                        final_pos.x = static_pos.x;
                    } else if !left_is_auto && width_is_auto && right_is_auto {
                        // width+right auto: position from left
                        let left = left_val.unwrap();
                        final_pos.x = containing_block_rect.origin.x + left + m_left;
                    } else if left_is_auto && !width_is_auto && !right_is_auto {
                        // left auto: solve for left
                        let right = right_val.unwrap();
                        let solved_left = cb_width - m_left - border_box_width - m_right - right;
                        final_pos.x = containing_block_rect.origin.x + solved_left + m_left;
                    } else if !left_is_auto && width_is_auto && !right_is_auto {
                        // width auto: position from left (width already resolved from content)
                        let left = left_val.unwrap();
                        final_pos.x = containing_block_rect.origin.x + left + m_left;
                    } else if !left_is_auto && !width_is_auto && right_is_auto {
                        // right auto: position from left
                        let left = left_val.unwrap();
                        final_pos.x = containing_block_rect.origin.x + left + m_left;
                    } else {
                        final_pos.x = static_pos.x;
                    }
                }
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
    calculated_positions: &mut super::PositionVec,
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
        let Some(current_pos) = calculated_positions.get_mut(node_index) else {
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
        let node_state = &ctx.styled_dom.styled_nodes.as_container()[node_dom_id].styled_node_state;

        use azul_css::props::style::StyleDirection;
        let direction = match get_direction_property(ctx.styled_dom, node_dom_id, node_state) {
            MultiValue::Exact(v) => v,
            _ => StyleDirection::Ltr,
        };
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

/// +spec:containing-block-p028 - §10.6.4: containing block for abspos is padding box of nearest positioned ancestor
/// Helper to find the containing block for an absolutely positioned element.
/// CSS 2.1 Section 10.1: The containing block for absolutely positioned elements
/// is the padding box of the nearest positioned ancestor.
///
/// Returns a `LogicalRect` representing the padding-box of the nearest
/// positioned ancestor, or the viewport (initial containing block) if none exists.
/// This is the unified entry point used by both sizing and positioning phases.
// +spec:containing-block-p001 +spec:containing-block-p016 +spec:containing-block-p019 +spec:containing-block-p032
// Containing block for absolutely positioned elements is established by
// nearest positioned ancestor (relative/absolute/fixed), or initial containing block if none.
pub fn find_absolute_containing_block_rect(
    tree: &LayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
    calculated_positions: &super::PositionVec,
    viewport: LogicalRect,
) -> Result<LogicalRect> {
    let mut current_parent_idx = tree.get(node_index).and_then(|n| n.parent);

    while let Some(parent_index) = current_parent_idx {
        let parent_node = tree.get(parent_index).ok_or(LayoutError::InvalidTree)?;

        // +spec:containing-block-p016 +spec:containing-block-p019 - nearest positioned ancestor (relative/absolute/fixed)
        if get_position_type(styled_dom, parent_node.dom_node_id) != LayoutPosition::Static {
            // calculated_positions stores margin-box positions
            let margin_box_pos = calculated_positions
                .get(parent_index)
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

    // +spec:containing-block-p001 +spec:containing-block-p016 +spec:containing-block-p019 +spec:containing-block-p032
    // No positioned ancestor found: fall back to initial containing block (viewport)
    Ok(viewport)
}
