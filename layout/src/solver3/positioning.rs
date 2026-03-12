//! solver3/positioning.rs
//! Pass 3: Final positioning of layout nodes
// +spec:positioning:79d47e - Implements relative, absolute, and fixed positioning schemes

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
            get_direction_property, get_display_property, get_writing_mode, get_position, MultiValue,
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

// +spec:positioning:94ef0f - position property: static|relative|absolute|sticky|fixed, initial static, applies to all elements except table-column-group/table-column
/// Looks up the `position` property using the compact-cache-aware getter.
// +spec:positioning:ba937d - positioned elements have position != static
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

// +spec:positioning:bda1d5 - resolves inset properties (top/right/bottom/left) as inward offsets per CSS Position 3 §3.1
// +spec:positioning:bf9168 - resolves inset properties (top/right/bottom/left) to control positioned box location
// +spec:positioning:f8e0a1 - inset properties (top/right/bottom/left) resolved for positioned elements; auto = unconstrained
/// **NEW API:** Correctly reads and resolves `top`, `right`, `bottom`, `left` properties,
/// including percentages relative to the containing block's size, and em/rem units.
/// Uses the modern resolve_with_context() API.
// +spec:positioning:7ec143 - top/right/bottom/left offset resolution with percentage against containing block
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

    // +spec:containing-block:d4b3b9 - percentage offsets resolve against CB width (left/right) or height (top/bottom)
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

// +spec:block-formatting-context:f5f992 - Out-of-flow: floated or absolutely positioned boxes laid out outside normal flow
// +spec:positioning:bb19f8 - absolute/fixed positioning: out-of-flow, positioned relative to containing block/viewport
/// After the main layout pass, this function iterates through the tree and correctly
/// calculates the final positions of out-of-flow elements (`absolute`, `fixed`).
// +spec:positioning:5bfef3 - abspos elements use static position for auto offsets, resolve against nearest positioned ancestor CB
// +spec:positioning:7fff75 - Absolute positioning: removed from flow, offset relative to containing block, establishes new CB
// +spec:positioning:839cbb - absolute elements positioned/sized solely relative to their containing block, modified by inset properties
// +spec:positioning:898590 - absolute positioning takes elements out of flow and positions them relative to containing block
// +spec:positioning:c37c1b - abspos boxes laid out in containing block after its final size is determined
// +spec:positioning:cbe481 - absolute positioning removes elements from flow and positions them relative to containing block
// +spec:positioning:ebff77 - absolute positioning layout model (replaces old §6 abspos model)
// +spec:positioning:3b3ba4 - Absolute positioning: box offset from containing block, removed from normal flow; fixed positioning: CB = viewport
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

        // +spec:positioning:1d87f6 - Fixed/absolute positioning schemes with box offset resolution (top/right/bottom/left)
        // +spec:positioning:8bde1d - absolute: out of flow, positioned by containing block
        // +spec:positioning:c11be9 - absolute positioning: effect of box offsets depends on which properties are auto (non-replaced) or intrinsic dimensions (replaced)
        // +spec:positioning:9020aa - "absolutely positioned" means position:absolute or position:fixed
        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            // is a grid container have their CB determined by grid-placement properties;
            // Taffy already handles this during grid layout, so skip re-positioning here.
            // Same applies to flex containers (Flexbox §4.1).
            {
                use azul_core::dom::FormattingContext;
                let parent_is_flex_or_grid = node.parent.and_then(|p| tree.get(p)).map_or(false, |pn| {
                    matches!(pn.formatting_context, FormattingContext::Flex | FormattingContext::Grid)
                });
                if parent_is_flex_or_grid {
                    continue;
                }
            }

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

            // +spec:containing-block:17a946 - fixed boxes use viewport as containing block
            // +spec:containing-block:83a32a - fixed positioning: containing block is viewport; absolute: nearest positioned ancestor or initial CB
            // +spec:containing-block:9b617d - fixed elements use viewport (initial fixed containing block)
            // +spec:containing-block:899e47 - fixed elements use viewport (initial fixed containing block)
            // +spec:containing-block:faa9a3 - fixed positioning falls back to initial containing block (viewport) when no ancestor establishes one
            // +spec:containing-block:faa9a3 - fixed positioning CB falls back to initial containing block (viewport) when no ancestor establishes one
            // +spec:positioning:067eab - CB for fixed = viewport, for absolute = nearest positioned ancestor
            // +spec:positioning:067eab - fixed CB is viewport; absolute CB is nearest positioned ancestor's padding-box
            // +spec:positioning:9777da - fixed positioning uses viewport as containing block
            // +spec:positioning:9777da - Fixed positioning uses viewport as containing block
            // +spec:positioning:9ccf9a - fixed-position CB is viewport (transform/will-change/contain could override, not yet implemented)
            // +spec:positioning:a68970 - fixed positioning uses viewport as containing block
            // +spec:positioning:8fff44 - fixed: same as absolute but positioned relative to viewport
            // +spec:positioning:f0ad47 - fixed elements use viewport as containing block; content outside viewport cannot be scrolled to
            // +spec:containing-block:df8387 - fixed positioning: containing block is the viewport
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
                    ctx.viewport_size,
                )?;

                // Store the calculated size in the tree node
                if let Some(node_mut) = tree.get_mut(node_index) {
                    node_mut.used_size = Some(size);
                }

                size
            };

            // +spec:positioning:dc23fa - sizing/positioning into inset-modified containing block (§4)
            // +spec:positioning:623e45 - inset properties reduce the containing block into the inset-modified containing block
            // Resolve offsets using the now-known containing block size.
            let offsets =
                resolve_position_offsets(ctx.styled_dom, Some(dom_id), containing_block_rect.size);

            // +spec:box-model:ae3899 - static position is the margin-edge position from normal flow
            // +spec:positioning:9a90a3 - static position: the position the element would have had in normal flow
            // +spec:positioning:ca3e89 - static-position rectangle uses block-start inline-start alignment (CSS2.1 hypothetical box)
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

            // +spec:box-model:ea2f43 - top + margin + border + padding + height + bottom = CB height
            // +spec:box-model:b4f5b3 - vertical constraint equation for abs-pos non-replaced elements
            // +spec:positioning:16d82c - vertical dimension constraint for abs-positioned non-replaced elements
            // +spec:positioning:8f474b - §10.6.4 vertical constraint for absolutely positioned non-replaced elements
            // +spec:positioning:50218d - absolute: top margin edge offset below containing block top edge
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
            // +spec:positioning:d730e5 - CB height is independent of the abspos element, so percentage heights always resolve
            let cb_height = containing_block_rect.size.height;

            let css_height = get_css_height(ctx.styled_dom, dom_id, node_state);
            let height_is_auto = css_height.is_auto();
            // +spec:overflow:941a06 - resolve auto inset properties: if only one is auto, solved to zero via constraint; if both auto, use static position
            let top_is_auto = offsets.top.is_none();
            let bottom_is_auto = offsets.bottom.is_none();

            // element_size is border-box (includes border + padding + content).
            // The constraint equation is:
            //   top + margin-top + border-box-height + margin-bottom + bottom = CB height
            // (border-top, padding-top, content-height, padding-bottom, border-bottom
            //  are all inside border-box-height)
            let mut used_height = element_size.height;
            // +spec:height-calculation:44939a - set auto values for margin-top/margin-bottom to 0
            // +spec:height-calculation:2f6e10 - if bottom is auto, replace auto margin-top/margin-bottom with 0
            let mut used_margin_top = if margin_auto.top { 0.0 } else { margin_top_val };
            let mut used_margin_bottom = if margin_auto.bottom { 0.0 } else { margin_bottom_val };

            // +spec:box-model:3a9c2a - resolving auto insets: static position fallback when insets are auto
            // +spec:box-model:bd442c - weaker inset resolves to align margin box with inset-modified CB edge
            // +spec:height-calculation:93e91c - abs non-replaced height: auto margin centering, single auto margin solve, over-constrained ignore bottom
            // +spec:positioning:6e7732 - §10.6.4 vertical constraint equation for abspos non-replaced elements
            // +spec:positioning:b63d0f - absolute positioning with top:auto uses static position (change bars example)
            // +spec:positioning:da8a0c - resolving auto insets: normal alignment treated as start, so auto insets resolve to static position
            // +spec:positioning:820b22 - 10.6.4: absolutely positioned non-replaced elements vertical constraint equation and 6 rules
            if top_is_auto && height_is_auto && bottom_is_auto {
                // +spec:positioning:08e0ac - absolute element with top:auto uses static position (current line)
                // +spec:positioning:aab294 - both inset properties auto: resolve to static position
                // +spec:positioning:d9bb3c - hypothetical position: UA may guess static position rather than fully computing hypothetical box
                // All three auto: set top to static position, height from content, solve for bottom
                // +spec:height-calculation:51627d - auto margins to 0, top = static position, height from content (rule 3)
                // +spec:positioning:460f2f - All three auto: set top to static position, height from content, solve for bottom
                final_pos.y = static_pos.y;
            } else if !top_is_auto && !height_is_auto && !bottom_is_auto {
                // +spec:overflow:fc0c9e - over-constrained abspos: auto margins minimize overflow (CSS2.1 equivalent of Box Alignment 3 safe alignment)
                // +spec:positioning:88f760 - auto margins of absolutely-positioned boxes (vertical)
                // None are auto: over-constrained case
                // +spec:height-calculation:03c071 - none auto: equal auto margins, solve single auto margin, or ignore bottom if over-constrained
                let top_val = offsets.top.unwrap();
                let bottom_val = offsets.bottom.unwrap();
                if margin_auto.top && margin_auto.bottom {
                    // +spec:height-calculation:5112a4 - both margin-top/bottom auto: solve with equal values
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
                // +spec:height-calculation:909b50 - top and height auto, bottom not auto: height from BFC auto heights, solve for top
                // Rule 1: height from content, auto margins to 0, solve for top
                let bottom_val = offsets.bottom.unwrap();
                let top_val = cb_height - used_margin_top - used_height - used_margin_bottom - bottom_val;
                final_pos.y = containing_block_rect.origin.y + top_val + used_margin_top;
            } else if top_is_auto && bottom_is_auto && !height_is_auto {
                // +spec:positioning:64e1ba - top+bottom auto, height not auto: set top to static position, solve for bottom
                final_pos.y = static_pos.y;
            } else if height_is_auto && bottom_is_auto && !top_is_auto {
                // Rule 3: height from content, auto margins to 0, solve for bottom
                let top_val = offsets.top.unwrap();
                final_pos.y = containing_block_rect.origin.y + top_val + used_margin_top;
            } else if top_is_auto && !height_is_auto && !bottom_is_auto {
                // +spec:height-calculation:33dce8 - top auto, height and bottom not auto: solve for top
                // Rule 4: auto margins to 0, solve for top
                let bottom_val = offsets.bottom.unwrap();
                let top_val = cb_height - used_margin_top - used_height - used_margin_bottom - bottom_val;
                final_pos.y = containing_block_rect.origin.y + top_val + used_margin_top;
            } else if height_is_auto && !top_is_auto && !bottom_is_auto {
                // +spec:positioning:62abb5 - automatic size resolved against inset-modified containing block
                // solve for height from constraint equation:
                // height = cb_height - top - margin_top - margin_bottom - bottom
                let top_val = offsets.top.unwrap();
                let bottom_val = offsets.bottom.unwrap();
                // +spec:containing-block:b3f0dd - clamp effective CB size to zero when insets exceed it (weaker inset reduced)
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

            // +spec:box-model:984243 - horizontal constraint equation for abs-pos non-replaced elements
            // +spec:positioning:3be194 - position abs replaced element after establishing width
            // Constraint: left + margin-left + border-left + padding-left + width +
            // +spec:width-calculation:1661b4 - constraint equation and six rules for abs-pos horizontal (§10.3.7)
            // left + margin-left + border-left + padding-left + width +
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
                    // +spec:positioning:88f760 - auto margins of absolutely-positioned boxes (horizontal)
                    // +spec:width-calculation:942c77 - abs-pos non-replaced width: auto margins, over-constrained resolution
                    // None of left/width/right are auto — solve for margins or handle over-constrained
                    // +spec:width-calculation:dff69d - §10.3.7 abs-pos non-replaced: none auto → equal auto margins, solve single auto margin, or over-constrained
                    let left = left_val.unwrap();
                    let right = right_val.unwrap();
                    let remaining = cb_width - left - border_box_width - right;

                    // +spec:writing-modes:9c3b40 - abspos auto margins: if negative remaining in inline axis, start margin=0, end margin gets remainder
                    if margin_left_auto && margin_right_auto {
                        // +spec:positioning:ab47b3 - auto margins can be negative in absolute positioning
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
                    // +spec:overflow:f323cb - auto inset: align margin box to stronger inset edge (may overflow CB)
                    // +spec:width-calculation:bbf97a - set auto margins to 0 for abspos when left/width/right has auto
                    // Set auto margins to 0, apply six rules
                    // +spec:box-model:2da091 - if either inset is auto, auto margins resolve to zero
                    // +spec:intrinsic-sizing:087b57 - abspos auto margins resolve to 0 when any inset is auto
                    // +spec:width-calculation:0c29ce - set auto margins to 0, then apply six rules for abs pos width
                    let m_left = if margin_left_auto { 0.0 } else { margin_left };
                    let m_right = if margin_right_auto { 0.0 } else { margin_right };

                    // +spec:width-calculation:2b2852 - all three auto: set auto margins to 0, use static position for left (LTR)
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

// +spec:positioning:5b0d7f - relative positioning: offset from normal flow position, siblings unaffected
// +spec:positioning:8afbe2 - Relative positioning preserves normal flow size and space; only visual offset applied after layout
/// +spec:positioning:3502d5 - relative and absolute positioning supported for combined use
// +spec:positioning:b22222 - relative positioning: offset from static position, purely visual effect
// +spec:positioning:b814b6 - relative/absolute/fixed positioning scheme (CSS Positioned Layout Module Level 3)
/// Final pass to shift relatively positioned elements from their static flow position.
/// +spec:block-formatting-context:60ccf9 - relative positioning shifts inline boxes as a unit after normal flow
/// +spec:display-property:17239f - relative positioning offsets element after normal flow; abspos elements taken out of flow
/// +spec:positioning:cbe066 - relative positioning implementation
///
/// This function now correctly resolves percentage-based offsets for `top`, `left`, etc.
/// According to the CSS spec, for relatively positioned elements, these percentages are
/// relative to the dimensions of the parent element's content box.
// +spec:positioning:2d8e15 - relative positioning shifts elements as a unit after normal flow without affecting surrounding content
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

        // +spec:block-formatting-context:faa1cf - static boxes: top/right/bottom/left do not apply
        // Early continue for non-relative positioning
        // +spec:overflow:cfb09a - Sticky positioning uses relative-like offsets, clamped to nearest scrollport at scroll time
        if position_type != LayoutPosition::Relative && position_type != LayoutPosition::Sticky {
            continue;
        }

        // +spec:table-layout:6cb73b - position:relative effect on table elements is undefined; skip them
        {
            use azul_css::props::layout::LayoutDisplay;
            let display = get_display_property(ctx.styled_dom, node.dom_node_id);
            if let MultiValue::Exact(d) = display {
                // +spec:positioning:4614dd - position does not apply to table-column-group or table-column boxes
                if matches!(
                    d,
                    LayoutDisplay::TableRowGroup
                        | LayoutDisplay::TableHeaderGroup
                        | LayoutDisplay::TableFooterGroup
                        | LayoutDisplay::TableRow
                        | LayoutDisplay::TableColumnGroup
                        | LayoutDisplay::TableColumn
                        | LayoutDisplay::TableCell
                        | LayoutDisplay::TableCaption
                ) {
                    continue;
                }
            }
        }

        // Determine the containing block size for resolving percentages.
        // For `position: relative`, this is the parent's content box size.
        let containing_block_size = node.parent
            .and_then(|parent_idx| tree.get(parent_idx))
            .map(|parent_node| {
                // Get parent's writing mode to correctly calculate its inner (content) size.
                let parent_wm = parent_node.dom_node_id
                    .map(|pid| {
                        let ps = &ctx.styled_dom.styled_nodes.as_container()[pid].styled_node_state;
                        get_writing_mode(ctx.styled_dom, pid, ps).unwrap_or_default()
                    })
                    .unwrap_or_default();
                let parent_used_size = parent_node.used_size.unwrap_or_default();
                parent_node.box_props.inner_size(parent_used_size, parent_wm)
            })
            // The root element is relatively positioned. Its containing block is the viewport.
            .unwrap_or(viewport.size);

        // +spec:positioning:418c74 - inset percentages resolve against containing block size per axis; auto is unconstrained
        let offsets =
            resolve_position_offsets(ctx.styled_dom, node.dom_node_id, containing_block_size);

        // Get a mutable reference to the position and apply the offsets.
        let Some(current_pos) = calculated_positions.get_mut(node_index) else {
            continue;
        };

        let initial_pos = *current_pos;

        // +spec:positioning:5eb813 - relative positioning offsets contents from normal flow position
        // +spec:positioning:a2e5f1 - relative positioning shifts element from static position (vs absolute/float)
        // top/bottom/left/right offsets are applied relative to the static position.
        let mut delta_x = 0.0;
        let mut delta_y = 0.0;

        // +spec:positioning:218b50 - Relative positioning: top=-bottom, left=-right, direction-dependent resolution, top wins over bottom
        // According to CSS 2.1 Section 9.4.3:
        // - For `top` and `bottom`: if both are specified, `top` wins and `bottom` is ignored
        // - For `left` and `right`: depends on direction (ltr/rtl)
        //   - In LTR: if both specified, `left` wins and `right` is ignored
        //   - In RTL: if both specified, `right` wins and `left` is ignored

        // +spec:overflow:53dffd - both left/right auto → used values are 0, boxes stay in original position
        // +spec:positioning:5a099e - negative offsets can cause overlapping (no clamping applied)
        // +spec:positioning:d189de - bottom offset for relative positioning is with respect to the box's own bottom edge
        // +spec:positioning:d80f47 - opposing inset values are negations: top wins over bottom, left/right per direction
        // +spec:positioning:ecc27c - relative positioning: left/right move box horizontally without changing size, left = -right
        // +spec:positioning:50218d - relative: offset from static position (top edges of box itself)
        // both auto → 0; one auto → negative of other; neither auto → bottom ignored (top wins)
        // +spec:positioning:ac768b - relative positioning: both auto→0, one auto→neg of other, neither→top wins; direction-aware left/right
        // +spec:positioning:e3727e - top/bottom: both auto→0, one auto→negative of other, neither auto→bottom ignored
        // Vertical positioning: `top` takes precedence over `bottom`
        if let Some(top) = offsets.top {
            delta_y = top;
        } else if let Some(bottom) = offsets.bottom {
            delta_y = -bottom;
        }

        // +spec:positioning:1732e8 - left/right for relatively positioned elements determined by 9.4.3 rules
        // Spec: "If the 'direction' property of the containing block is 'ltr', the value of 'left' wins"
        // Get the direction of the containing block (parent), not the element itself
        use azul_css::props::style::StyleDirection;
        let cb_direction = node.parent
            .and_then(|parent_idx| tree.get(parent_idx))
            .and_then(|parent_node| {
                let parent_dom_id = parent_node.dom_node_id?;
                let parent_state =
                    &ctx.styled_dom.styled_nodes.as_container()[parent_dom_id].styled_node_state;
                match get_direction_property(ctx.styled_dom, parent_dom_id, parent_state) {
                    MultiValue::Exact(v) => Some(v),
                    _ => None,
                }
            })
            .unwrap_or(StyleDirection::Ltr);
        // +spec:containing-block:6d4fb1 - over-constrained relative positioning: ltr→left wins, rtl→right wins
        match cb_direction {
            StyleDirection::Ltr => {
                if let Some(left) = offsets.left {
                    delta_x = left;
                } else if let Some(right) = offsets.right {
                    // +spec:overflow:fb426c - left auto: used value is minus the value of right
                    delta_x = -right;
                }
            }
            StyleDirection::Rtl => {
                if let Some(right) = offsets.right {
                    delta_x = -right;
                } else if let Some(left) = offsets.left {
                    delta_x = left;
                }
            }
        }

        // +spec:overflow:f1e1ce - relative positioning may cause overflow:auto/scroll boxes to need scrollbars
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

// +spec:positioning:22f165 - absolute/fixed containing block: nearest positioned ancestor's padding-box, or initial CB
/// Helper to find the containing block for an absolutely positioned element.
/// CSS 2.1 Section 10.1: The containing block for absolutely positioned elements
/// is the padding box of the nearest positioned ancestor.
// +spec:containing-block:10af51 - absolutely positioned element's CB is nearest positioned ancestor
// +spec:positioning:2d0dbb - containing block for abspos is padding-box of nearest positioned ancestor, or initial CB
// +spec:positioning:3ac06c - abspos positioned relative to containing block ignoring fragmentation breaks
// +spec:positioning:d7e4b4 - containing block of abspos element is always definite (returns concrete LogicalRect)
// +spec:positioning:fc9dba - containing block resolution for absolutely positioned boxes
///
/// Returns a `LogicalRect` representing the padding-box of the nearest
/// positioned ancestor, or the viewport (initial containing block) if none exists.
/// This is the unified entry point used by both sizing and positioning phases.
// +spec:containing-block:18ae8e - Absolute positioning: abs-pos box establishes new CB for normal flow and abs-pos (but not fixed) descendants
// +spec:containing-block:b6cb8b - containing block for abs-pos is nearest positioned ancestor
// +spec:display-property:5a39bc - containing block for abspos is nearest positioned ancestor or initial containing block
// +spec:positioning:09a0fa - Absolute positioning: CB is padding-box of nearest positioned ancestor
// +spec:positioning:467cb1 - Containing block for abs pos = nearest positioned ancestor or initial CB
// +spec:positioning:99d0bb - containing block for absolute elements is nearest positioned ancestor
// +spec:positioning:92e099 - containing block for abs pos is nearest positioned ancestor or initial CB
// +spec:positioning:f57523 - containing block of abspos element is always definite (returns concrete LogicalRect)
// +spec:width-calculation:bf1aa6 - abspos CB is nearest positioned ancestor, else initial CB
// Containing block for absolutely positioned elements is established by
// nearest positioned ancestor (relative/absolute/fixed), or initial containing block if none.
// +spec:positioning:8f50de - relatively positioned parent serves as containing block for abspos descendants
// +spec:containing-block:6bcb0c - containing block is padding edge of nearest positioned ancestor, or initial containing block if none
// +spec:containing-block:bf17e5 - containing block for abspos is padding box of nearest positioned ancestor, or initial CB
// +spec:containing-block:d0f92d - containing block for positioned box is nearest positioned ancestor, or initial containing block
// +spec:containing-block:d7e013 - containing block for positioned box is nearest positioned ancestor or initial CB
// +spec:containing-block:05bc0d - positioning an element changes which ancestor establishes the CB for its descendants
// +spec:positioning:355ee4 - CB for abspos is padding edge of nearest positioned ancestor, or initial CB
// +spec:positioning:383794 - Containing block for abspos is nearest positioned ancestor, or initial containing block if none
// +spec:positioning:5b3e43 - Containing block for abs-pos is padding box of nearest positioned ancestor, or initial CB
// +spec:positioning:882e67 - containing block for abs pos is nearest positioned ancestor or initial CB
// +spec:positioning:292c5c - relative parent serves as containing block for absolute descendants
pub fn find_absolute_containing_block_rect(
    tree: &LayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
    calculated_positions: &super::PositionVec,
    viewport: LogicalRect,
) -> Result<LogicalRect> {
    let mut current_parent_idx = tree.get(node_index).and_then(|n| n.parent);

    // +spec:positioning:aa361e - values other than static make a box positioned and establish an abspos containing block
    while let Some(parent_index) = current_parent_idx {
        let parent_node = tree.get(parent_index).ok_or(LayoutError::InvalidTree)?;

        if get_position_type(styled_dom, parent_node.dom_node_id) != LayoutPosition::Static {
            // calculated_positions stores margin-box positions
            let margin_box_pos = calculated_positions
                .get(parent_index)
                .copied()
                .unwrap_or_default();
            // used_size is the border-box size
            let border_box_size = parent_node.used_size.unwrap_or_default();

            // +spec:containing-block:6bcb0c - containing block formed by padding edge of nearest positioned ancestor
            // +spec:positioning:df1921 - abs-pos percentage widths resolve against padding box of containing block
            // Calculate padding-box origin (margin-box + border)
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

    // +spec:positioning:3d88c9 - abspos available space is always definite (viewport or positioned ancestor padding box)
    // No positioned ancestor found: fall back to initial containing block (viewport)
    // +spec:containing-block:141dcc - absolute element with no positioned ancestor uses initial containing block
    // +spec:containing-block:657f2f - containing block becomes initial containing block when no positioned ancestors
    // +spec:containing-block:7f5090 - if no ancestor establishes one, absolute positioning CB is initial containing block
    // +spec:containing-block:7f5090 - fallback to initial containing block when no positioned ancestor
    // +spec:containing-block:ad5ebc - no positioned ancestor: containing block becomes the initial containing block
    // +spec:display-property:813192 - abspos containing block falls back to initial containing block (viewport) when no positioned ancestor
    Ok(viewport)
}
