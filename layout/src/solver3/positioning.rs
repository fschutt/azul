//! Final positioning of layout nodes (relative, absolute, and fixed schemes)
// +spec:positioning:79d47e - Implements relative, absolute, and fixed positioning schemes

use crate::debug_log;
use std::collections::BTreeMap;

use azul_core::{
    dom::{NodeId, NodeType},
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
    font_traits::{FontLoaderTrait, ParsedFontTrait, TextLayoutCache},
    solver3::{
        fc::{layout_formatting_context, FloatingContext, LayoutConstraints, TextAlign},
        getters::{
            get_aspect_ratio_property, get_direction_property, get_display_property, get_writing_mode, get_position, MultiValue,
            get_css_top, get_css_bottom, get_css_left, get_css_right,
            get_css_height, get_css_width,
        },
        layout_tree::LayoutTree,
        LayoutContext, LayoutError, Result,
    },
};

#[derive(Debug, Default)]
pub(crate) struct PositionOffsets {
    pub(crate) top: Option<f32>,
    pub(crate) right: Option<f32>,
    pub(crate) bottom: Option<f32>,
    pub(crate) left: Option<f32>,
}

// +spec:positioning:94ef0f - position property: static|relative|absolute|sticky|fixed, initial static, applies to all elements except table-column-group/table-column
/// Looks up the `position` property using the compact-cache-aware getter.
// +spec:positioning:ba937d - positioned elements have position != static
#[must_use] pub fn get_position_type(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutPosition {
    let Some(id) = dom_id else {
        return LayoutPosition::Static;
    };
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    get_position(styled_dom, id, node_state).unwrap_or_default()
}

// +spec:positioning:bda1d5 - resolves inset properties (top/right/bottom/left) as inward offsets per CSS Position 3 §3.1
// +spec:positioning:bf9168 - resolves inset properties (top/right/bottom/left) to control positioned box location
// +spec:positioning:f8e0a1 - inset properties (top/right/bottom/left) resolved for positioned elements; auto = unconstrained
/// Reads and resolves `top`, `right`, `bottom`, `left` properties,
/// including percentages relative to the containing block's size, and em/rem units.
// +spec:positioning:7ec143 - top/right/bottom/left offset resolution with percentage against containing block
#[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a struct literal is not clearer here
pub(crate) fn resolve_position_offsets(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
    cb_size: LogicalSize,
    viewport_size: LogicalSize,
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
        viewport_size: PhysicalSize::new(viewport_size.width, viewport_size.height),
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
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Panics
///
/// Panics if a resolved offset (`top`/`bottom`) is None where both edges are expected.
pub fn position_out_of_flow_elements<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    calculated_positions: &mut super::PositionVec,
    viewport: LogicalRect,
) {
    use azul_css::props::style::StyleDirection;
    // Returns `()` (not Result<()>): inner fallible calls use skip-on-err (see above), so this fn
    // never propagates Err. Avoids the lift-fragile Result<(),LayoutError> Ok-niche read.
    for node_index in 0..tree.nodes.len() {
        let node = &tree.nodes[node_index];
        let Some(dom_id) = node.dom_node_id else {
            continue;
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
                let parent_is_flex_or_grid = node.parent.and_then(|p| tree.get(p)).is_some_and(|pn| {
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
                            let pbp = parent_node.box_props.unpack();
                            (
                                parent_idx,
                                *parent_pos,
                                pbp.border.left,
                                pbp.border.top,
                                pbp.padding.left,
                                pbp.padding.top,
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
            // +spec:positioning:744713 - fixed position uses viewport as containing block
            // +spec:positioning:f0ad47 - fixed elements use viewport as containing block; content outside viewport cannot be scrolled to
            // +spec:containing-block:df8387 - fixed positioning: containing block is the viewport
            let containing_block_rect = if position_type == LayoutPosition::Fixed {
                viewport
            } else {
                // skip-on-err (was `?`): a CB-resolution failure for one out-of-flow node skips
                // that node rather than aborting the whole layout. Lets this fn return `()`
                // (no Result<(),LayoutError> Ok-niche read, which the remill→wasm lift mis-lowers).
                match find_absolute_containing_block_rect(
                    tree,
                    node_index,
                    ctx.styled_dom,
                    calculated_positions,
                    viewport,
                ) {
                    Ok(r) => r,
                    Err(_) => continue,
                }
            };

            // Get node again after containing block calculation
            let node = &tree.nodes[node_index];

            // Calculate used size for out-of-flow elements (they don't get sized during normal
            // layout)
            let element_size = if let Some(size) = node.used_size {
                size
            } else {
                // Element hasn't been sized yet - calculate it now using containing block
                let intrinsic = tree.warm(node_index).and_then(|w| w.intrinsic_sizes).unwrap_or_default();
                let Ok(size) = crate::solver3::sizing::calculate_used_size_for_node(
                    ctx.styled_dom,
                    Some(dom_id),
                    &containing_block_rect.size,
                    intrinsic,
                    &node.box_props.unpack(),
                    &ctx.viewport_size,
                ) else {
                    continue;
                };

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
                resolve_position_offsets(ctx.styled_dom, Some(dom_id), containing_block_rect.size, viewport.size);

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
                let nbp = node.box_props.unpack();
                (nbp.margin.top, nbp.margin.bottom,
                 nbp.margin_auto,
                 nbp.margin.left, nbp.margin.right,
                 nbp.margin_auto.left, nbp.margin_auto.right)
            };
            // +spec:positioning:d730e5 - CB height is independent of the abspos element, so percentage heights always resolve
            let cb_height = containing_block_rect.size.height;

            let css_height = get_css_height(ctx.styled_dom, dom_id, node_state);
            // +spec:replaced-elements:7d8ba8 - §10.6.5: for absolutely positioned replaced
            // elements, height is determined first (as for inline replaced elements), so treat
            // it as "not auto" in the constraint equation even if CSS says auto.
            let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
            let is_replaced = matches!(node_data.node_type, NodeType::Image(_))
                || node_data.is_virtual_view_node();
            let height_is_auto = css_height.is_auto() && !is_replaced;
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
                // +spec:intrinsic-sizing:566a43 - abspos auto height with non-auto insets: stretch-fit size
                // +spec:intrinsic-sizing:c7227f - except: if box has aspect-ratio, ratio-dependent axis uses max-content
                let has_aspect_ratio = matches!(
                    get_aspect_ratio_property(ctx.styled_dom, dom_id, node_state),
                    MultiValue::Exact(azul_css::props::style::effects::StyleAspectRatio::Ratio(_))
                );
                let top_val = offsets.top.unwrap();
                let bottom_val = offsets.bottom.unwrap();
                if !has_aspect_ratio {
                    // solve for height from constraint equation (stretch-fit):
                    // height = cb_height - top - margin_top - margin_bottom - bottom
                    // +spec:containing-block:b3f0dd - clamp effective CB size to zero when insets exceed it (weaker inset reduced)
                    used_height = (cb_height - top_val - used_margin_top - used_margin_bottom - bottom_val).max(0.0);
                }
                // else: keep content-based height (max-content) per aspect-ratio exception
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
                let cb_direction = {
                    let cb_dom_id = if position_type == LayoutPosition::Fixed {
                        None // viewport CB, default LTR
                    } else {
                        let mut parent = tree.nodes[node_index].parent;
                        let mut found = None;
                        while let Some(pidx) = parent {
                            if let Some(pnode) = tree.get(pidx) {
                                if get_position_type(ctx.styled_dom, pnode.dom_node_id).is_positioned() {
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

                // +spec:replaced-elements:7d8ba8 - §10.3.8: for absolutely positioned replaced elements, width is determined
                // first (as for inline replaced), so treat as "not auto" in the constraint.
                let width_is_auto = get_css_width(ctx.styled_dom, dom_id, node_state).is_auto() && !is_replaced;

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
                    // +spec:width-calculation:c120b3 - all three of left/width/right auto: set auto margins to 0, then use direction to pick static position
                    if left_is_auto && width_is_auto && right_is_auto {
                        match cb_direction {
                            StyleDirection::Ltr => {
                                // Set left to static position, apply rule 3 (width from content, solve for right)
                                final_pos.x = static_pos.x;
                            }
                            StyleDirection::Rtl => {
                                // Set right to static position, apply rule 1 (width from content, solve for left)
                                let static_offset = static_pos.x - containing_block_rect.origin.x;
                                let right_static = (cb_width - static_offset - border_box_width).max(0.0);
                                let solved_left = cb_width - m_left - border_box_width - m_right - right_static;
                                final_pos.x = containing_block_rect.origin.x + solved_left + m_left;
                            }
                        }
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
                        // +spec:intrinsic-sizing:566a43 - abspos auto width with non-auto insets: stretch-fit size
                        // +spec:intrinsic-sizing:c7227f - except: if box has aspect-ratio, ratio-dependent axis uses max-content
                        let has_aspect_ratio = matches!(
                            get_aspect_ratio_property(ctx.styled_dom, dom_id, node_state),
                            MultiValue::Exact(azul_css::props::style::effects::StyleAspectRatio::Ratio(_))
                        );
                        let left = left_val.unwrap();
                        let right = right_val.unwrap();
                        if !has_aspect_ratio {
                            // width = cb_width - left - margin_left - margin_right - right
                            let used_width = (cb_width - left - m_left - m_right - right).max(0.0);
                            if let Some(node_mut) = tree.get_mut(node_index) {
                                if let Some(ref mut size) = node_mut.used_size {
                                    size.width = used_width;
                                }
                            }
                        }
                        // else: keep content-based width (max-content) per aspect-ratio exception
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

            super::pos_set(calculated_positions, node_index, final_pos);

            // The absolute box is now at its FINAL, definite size. Lay out its
            // content against that box if a percentage-height child collapsed —
            // which happens because (a) the taffy-bridge layout path that handles
            // flex-nested blocks never runs `process_out_of_flow_children`, so an
            // abs child's subtree is otherwise NEVER laid out, and (b) even on the
            // solver3 path the subtree is laid out BEFORE the stretch-fit height is
            // resolved here. Either way the child saw a 0-height containing block.
            // Re-flowing now (the abs height is independent of its content, so this
            // can't loop) lets `height:100%` children resolve against the real box.
            // (Root cause of the slippy-map VirtualView blank-bounds bug.)
            if height_is_auto {
                let (used_size, inner, child_collapsed) = {
                    let n = &tree.nodes[node_index];
                    let used = n.used_size.unwrap_or_default();
                    let inner = n.box_props.inner_size(used, LayoutWritingMode::HorizontalTb);
                    let collapsed = inner.height > 1.0
                        && tree.children(node_index).iter().any(|&c| {
                            tree.get(c)
                                .and_then(|cn| cn.used_size)
                                .is_none_or(|s| s.height < 1.0)
                        });
                    (used, inner, collapsed)
                };
                let _ = used_size;
                if child_collapsed {
                    let constraints = LayoutConstraints {
                        available_size: inner,
                        writing_mode: LayoutWritingMode::HorizontalTb,
                        writing_mode_ctx: super::geometry::WritingModeContext::default(),
                        bfc_state: None,
                        text_align: TextAlign::Start,
                        containing_block_size: inner,
                        available_width_type:
                            crate::text3::cache::AvailableSpace::Definite(inner.width),
                    };
                    let mut reflow_float_cache: std::collections::HashMap<usize, FloatingContext> =
                        std::collections::HashMap::new();
                    drop(layout_formatting_context(
                        ctx,
                        tree,
                        text_cache,
                        node_index,
                        &constraints,
                        &mut reflow_float_cache,
                    ));
                }
            }
        }
    }
}

// +spec:positioning:5b0d7f - relative positioning: offset from normal flow position, siblings unaffected
// +spec:positioning:8afbe2 - Relative positioning preserves normal flow size and space; only visual offset applied after layout
// +spec:positioning:3502d5 - relative and absolute positioning supported for combined use
// +spec:positioning:b22222 - relative positioning: offset from static position, purely visual effect
// +spec:positioning:b814b6 - relative/absolute/fixed positioning scheme (CSS Positioned Layout Module Level 3)
/// Final pass to shift relatively positioned elements from their static flow position.
// +spec:block-formatting-context:60ccf9 - relative positioning shifts inline boxes as a unit after normal flow
// +spec:display-property:17239f - relative positioning offsets element after normal flow; abspos elements taken out of flow
// +spec:positioning:cbe066 - relative positioning implementation
///
/// Resolves percentage-based offsets for `top`, `left`, etc.
/// For relatively positioned elements, percentages are
/// relative to the dimensions of the parent element's content box.
// +spec:positioning:2d8e15 - relative positioning shifts elements as a unit after normal flow without affecting surrounding content
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
pub fn adjust_relative_positions<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    calculated_positions: &mut super::PositionVec,
    viewport: LogicalRect, // The viewport is needed if the root element is relative.
) {
    use azul_css::props::style::StyleDirection;
    // NOTE: returns `()` (not `Result<()>`). This fn is Ok-always — its only `?` are on `Option`
    // inside `.and_then` closures, never propagating to the fn body. The previous `Result<(),
    // LayoutError>` return forced the `?` at the call site to read an Ok-niche discriminant, which
    // the remill→wasm lift mis-lowers (per-build, ASLR-dependent) → a FALSE Err that aborted the
    // whole layout in the web backend (rect=0). Matches sibling reposition_* fns that return ().
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
        // +spec:table-layout:718f91 - relative positioning on table-row/row-group shifts all contents
        {
            use azul_css::props::layout::LayoutDisplay;
            let display = get_display_property(ctx.styled_dom, node.dom_node_id);
            if let MultiValue::Exact(d) = display {
                // +spec:positioning:4614dd - position does not apply to table-column-group or table-column boxes
                // Table-row and row-group elements DO support relative positioning:
                // the shift affects all contents including cells originating in the row.
                // Table-column, table-column-group, table-cell, and table-caption do not.
                if matches!(
                    d,
                    LayoutDisplay::TableColumnGroup
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
            .map_or(viewport.size, |parent_node| {
                // Get parent's writing mode to correctly calculate its inner (content) size.
                let parent_wm = parent_node.dom_node_id
                    .map(|pid| {
                        let ps = &ctx.styled_dom.styled_nodes.as_container()[pid].styled_node_state;
                        get_writing_mode(ctx.styled_dom, pid, ps).unwrap_or_default()
                    })
                    .unwrap_or_default();
                let parent_used_size = parent_node.used_size.unwrap_or_default();
                parent_node.box_props.inner_size(parent_used_size, parent_wm)
            });

        // +spec:positioning:418c74 - inset percentages resolve against containing block size per axis; auto is unconstrained
        let offsets =
            resolve_position_offsets(ctx.styled_dom, node.dom_node_id, containing_block_size, viewport.size);

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

            debug_log!(ctx, "Adjusted relative element #{} from {:?} to {:?} (delta: {}, {})",
                node_index, initial_pos, *current_pos, delta_x, delta_y);

            // +spec:table-layout:ec2600 - For table-row-group, table-header-group, table-footer-group, or table-row,
            // the relative shift affects all contents of the box including table cells.
            // Propagate the delta to all descendant nodes.
            {
                use azul_css::props::layout::LayoutDisplay;
                let display = get_display_property(ctx.styled_dom, node.dom_node_id);
                let is_table_row_like = matches!(
                    display,
                    MultiValue::Exact(
                        LayoutDisplay::TableRowGroup
                        | LayoutDisplay::TableHeaderGroup
                        | LayoutDisplay::TableFooterGroup
                        | LayoutDisplay::TableRow
                    )
                );
                if is_table_row_like {
                    // Shift all children (and their descendants) by the same delta
                    let mut stack = tree.children(node_index).to_vec();
                    while let Some(child_idx) = stack.pop() {
                        if let Some(child_pos) = calculated_positions.get_mut(child_idx) {
                            child_pos.x += delta_x;
                            child_pos.y += delta_y;
                        }
                        stack.extend_from_slice(tree.children(child_idx));
                    }
                }
            }
        }
    }
}

// +spec:overflow:bac4e5 - sticky view rectangle from inset properties relative to nearest scrollport

/// Finds the nearest scrollport (ancestor with overflow: scroll or auto) for a node.
/// Returns the content-box rect of the scrollport, or the viewport if none found.
fn find_nearest_scrollport(
    tree: &LayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
    calculated_positions: &super::PositionVec,
    viewport: LogicalRect,
) -> LogicalRect {
    use crate::solver3::getters::{get_overflow_x, get_overflow_y};
    use azul_css::props::layout::LayoutOverflow;

    let mut current_parent_idx = tree.get(node_index).and_then(|n| n.parent);

    while let Some(parent_index) = current_parent_idx {
        let Some(parent_node) = tree.get(parent_index) else {
            break;
        };
        let Some(parent_dom_id) = parent_node.dom_node_id else {
            current_parent_idx = parent_node.parent;
            continue;
        };

        let node_state = &styled_dom.styled_nodes.as_container()[parent_dom_id].styled_node_state;
        let ox = get_overflow_x(styled_dom, parent_dom_id, node_state);
        let oy = get_overflow_y(styled_dom, parent_dom_id, node_state);

        let is_scrollport = matches!(
            ox,
            MultiValue::Exact(LayoutOverflow::Scroll | LayoutOverflow::Auto)
        ) || matches!(
            oy,
            MultiValue::Exact(LayoutOverflow::Scroll | LayoutOverflow::Auto)
        );

        if is_scrollport {
            let margin_box_pos = calculated_positions
                .get(parent_index)
                .copied()
                .unwrap_or_default();
            let border_box_size = parent_node.used_size.unwrap_or_default();

            // Content-box = margin-box pos + border + padding, size - border - padding
            let pbp = parent_node.box_props.unpack();
            let content_pos = LogicalPosition::new(
                margin_box_pos.x
                    + pbp.border.left
                    + pbp.padding.left,
                margin_box_pos.y
                    + pbp.border.top
                    + pbp.padding.top,
            );
            let content_size = LogicalSize::new(
                (border_box_size.width
                    - pbp.border.left
                    - pbp.border.right
                    - pbp.padding.left
                    - pbp.padding.right)
                    .max(0.0),
                (border_box_size.height
                    - pbp.border.top
                    - pbp.border.bottom
                    - pbp.padding.top
                    - pbp.padding.bottom)
                    .max(0.0),
            );
            return LogicalRect::new(content_pos, content_size);
        }

        current_parent_idx = parent_node.parent;
    }

    viewport
}

/// Find the scroll offset of the nearest scroll container ancestor.
/// Returns the scroll offset as a `LogicalPosition` (how far the content has scrolled).
fn find_nearest_scroll_offset(
    tree: &LayoutTree,
    node_index: usize,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
) -> LogicalPosition {
    let mut parent = tree.get(node_index).and_then(|n| n.parent);
    while let Some(pidx) = parent {
        if let Some(pnode) = tree.get(pidx) {
            if let Some(dom_id) = pnode.dom_node_id {
                if let Some(scroll_pos) = scroll_offsets.get(&dom_id) {
                    let offset_x = scroll_pos.children_rect.origin.x - scroll_pos.parent_rect.origin.x;
                    let offset_y = scroll_pos.children_rect.origin.y - scroll_pos.parent_rect.origin.y;
                    return LogicalPosition::new(offset_x, offset_y);
                }
            }
            parent = pnode.parent;
        } else {
            break;
        }
    }
    LogicalPosition::zero()
}

/// Adjusts positions of sticky-positioned elements based on scroll offset.
///
/// Sticky positioning works like relative positioning, but the element's position
/// is constrained by its inset properties (top/right/bottom/left) relative to the
/// nearest scrollport (scroll container ancestor). The margin box is further
/// constrained to remain within the containing block.
///
/// +spec:position-sticky:9449f1 - for sticky positioning, insets represent offsets from scrollport edge
/// +spec:position-sticky:75412d - multiple sticky boxes in same container offset independently
/// +spec:box-model:af9af8 - sticky positioning: shift element to stay within sticky view rectangle, margin box constrained to containing block
/// +spec:overflow:bac4e5 - compute sticky view rectangle, clamp end-edge insets to border box size
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
pub fn adjust_sticky_positions<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    calculated_positions: &mut super::PositionVec,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    viewport: LogicalRect,
) {
    // Returns `()` (not `Result<()>`): Ok-always (its only `?` is Option-`?` in an `.and_then`
    // closure). Avoids the lift-fragile Result<(),LayoutError> Ok-niche read at the call site.
    for node_index in 0..tree.nodes.len() {
        let node = &tree.nodes[node_index];
        let position_type = get_position_type(ctx.styled_dom, node.dom_node_id);

        if position_type != LayoutPosition::Sticky {
            continue;
        }

        let Some(dom_id) = node.dom_node_id else {
            continue;
        };

        // Find the nearest scrollport for this sticky element
        let scrollport = find_nearest_scrollport(
            tree,
            node_index,
            ctx.styled_dom,
            calculated_positions,
            viewport,
        );

        // The containing block for percentage resolution is the parent's content box
        let containing_block = node.parent
            .and_then(|parent_idx| {
                let parent_node = tree.get(parent_idx)?;
                let parent_pos = calculated_positions.get(parent_idx).copied().unwrap_or_default();
                let parent_size = parent_node.used_size.unwrap_or_default();
                let parent_wm = parent_node.dom_node_id
                    .map(|pid| {
                        let ps = &ctx.styled_dom.styled_nodes.as_container()[pid].styled_node_state;
                        get_writing_mode(ctx.styled_dom, pid, ps).unwrap_or_default()
                    })
                    .unwrap_or_default();
                let pbp = parent_node.box_props.unpack();
                let content_size = pbp.inner_size(parent_size, parent_wm);
                let content_origin = LogicalPosition::new(
                    parent_pos.x + pbp.border.left + pbp.padding.left,
                    parent_pos.y + pbp.border.top + pbp.padding.top,
                );
                Some(LogicalRect::new(content_origin, content_size))
            })
            .unwrap_or(viewport);

        // Resolve inset properties (top, right, bottom, left)
        let offsets = resolve_position_offsets(ctx.styled_dom, Some(dom_id), scrollport.size, viewport.size);

        // Get the scroll offset from the nearest scroll container
        let scroll_offset = find_nearest_scroll_offset(tree, node_index, scroll_offsets);

        let Some(current_pos) = calculated_positions.get_mut(node_index) else {
            continue;
        };

        let static_pos = *current_pos;
        let element_size = node.used_size.unwrap_or_default();
        let nbp = node.box_props.unpack();
        let margin = &nbp.margin;

        let mut shift_x = 0.0f32;
        let mut shift_y = 0.0f32;

        // For each side: if inset is not auto, clamp the border edge to stay
        // within the sticky view rectangle (scrollport inset by the specified amount).
        // The scroll offset shifts the effective scrollport position.
        if let Some(top_inset) = offsets.top {
            let sticky_edge = scrollport.origin.y + scroll_offset.y + top_inset;
            let border_top = current_pos.y;
            if border_top < sticky_edge {
                shift_y = shift_y.max(sticky_edge - border_top);
            }
        }

        if let Some(bottom_inset) = offsets.bottom {
            let sticky_edge = scrollport.origin.y + scroll_offset.y + scrollport.size.height - bottom_inset;
            let border_bottom = current_pos.y + element_size.height;
            if border_bottom > sticky_edge {
                shift_y = shift_y.min(sticky_edge - border_bottom);
            }
        }

        if let Some(left_inset) = offsets.left {
            let sticky_edge = scrollport.origin.x + scroll_offset.x + left_inset;
            let border_left = current_pos.x;
            if border_left < sticky_edge {
                shift_x = shift_x.max(sticky_edge - border_left);
            }
        }

        if let Some(right_inset) = offsets.right {
            let sticky_edge = scrollport.origin.x + scroll_offset.x + scrollport.size.width - right_inset;
            let border_right = current_pos.x + element_size.width;
            if border_right > sticky_edge {
                shift_x = shift_x.min(sticky_edge - border_right);
            }
        }

        // Constrain: the margin box must remain within the containing block
        if shift_y != 0.0 {
            let margin_box_top = current_pos.y - margin.top + shift_y;
            let margin_box_bottom = current_pos.y + element_size.height + margin.bottom + shift_y;
            if margin_box_top < containing_block.origin.y {
                shift_y += containing_block.origin.y - margin_box_top;
            }
            let cb_bottom = containing_block.origin.y + containing_block.size.height;
            if margin_box_bottom > cb_bottom {
                shift_y -= margin_box_bottom - cb_bottom;
            }
        }

        if shift_x != 0.0 {
            let margin_box_left = current_pos.x - margin.left + shift_x;
            let margin_box_right = current_pos.x + element_size.width + margin.right + shift_x;
            if margin_box_left < containing_block.origin.x {
                shift_x += containing_block.origin.x - margin_box_left;
            }
            let cb_right = containing_block.origin.x + containing_block.size.width;
            if margin_box_right > cb_right {
                shift_x -= margin_box_right - cb_right;
            }
        }

        if shift_x != 0.0 || shift_y != 0.0 {
            current_pos.x += shift_x;
            current_pos.y += shift_y;

            debug_log!(ctx, "Adjusted sticky element #{} from {:?} to {:?}",
                node_index, static_pos, *current_pos);
        }
    }
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
// +spec:positioning:00ce38 - CB for absolute is padding edge of nearest positioned ancestor
pub(crate) fn find_absolute_containing_block_rect(
    tree: &LayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
    calculated_positions: &super::PositionVec,
    viewport: LogicalRect,
) -> Result<LogicalRect> {
    // +spec:positioning:748d87 - walk up to nearest positioned ancestor for CB
    let mut current_parent_idx = tree.get(node_index).and_then(|n| n.parent);

    // +spec:positioning:aa361e - values other than static make a box positioned and establish an abspos containing block
    while let Some(parent_index) = current_parent_idx {
        let parent_node = tree.get(parent_index).ok_or(LayoutError::InvalidTree)?;

        if get_position_type(styled_dom, parent_node.dom_node_id).is_positioned() {
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
            let pbp = parent_node.box_props.unpack();
            let padding_box_pos = LogicalPosition::new(
                margin_box_pos.x + pbp.border.left,
                margin_box_pos.y + pbp.border.top,
            );

            // Calculate padding-box size (border-box - borders)
            let padding_box_size = LogicalSize::new(
                (border_box_size.width
                    - pbp.border.left
                    - pbp.border.right)
                    .max(0.0),
                (border_box_size.height
                    - pbp.border.top
                    - pbp.border.bottom)
                    .max(0.0),
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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use azul_core::dom::{Dom, FormattingContext, IdOrClass};

    use super::*;
    use crate::solver3::{
        geometry::{EdgeSizes, MarginAuto, PackedBoxProps, ResolvedBoxProps},
        layout_tree::{LayoutNodeCold, LayoutNodeHot, LayoutNodeWarm},
        pos_set, PositionVec, POSITION_UNSET,
    };

    // ==================================================================
    // Fixtures
    // ==================================================================

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn viewport() -> LogicalRect {
        LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(800.0, 600.0),
        )
    }

    fn styled(dom: Dom, css_str: &str) -> StyledDom {
        let mut dom = dom;
        let (css, _warnings) = azul_css::parser2::new_from_str(css_str);
        StyledDom::create(&mut dom, css)
    }

    fn div_class(class: &str) -> Dom {
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
    }

    fn body_class(class: &str) -> Dom {
        Dom::create_body().with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
    }

    /// Structural lookup — never hard-code `CompactDom` pre-order indices.
    fn node_by_class(sd: &StyledDom, class: &str) -> NodeId {
        let container = sd.node_data.as_container();
        for i in 0..sd.node_data.len() {
            let id = NodeId::new(i);
            let ids_and_classes = container[id].get_ids_and_classes();
            let hit = ids_and_classes
                .as_ref()
                .iter()
                .any(|ioc| matches!(ioc, IdOrClass::Class(c) if c.as_str() == class));
            if hit {
                return id;
            }
        }
        panic!("no node with class {class:?}");
    }

    fn edges(top: f32, right: f32, bottom: f32, left: f32) -> EdgeSizes {
        EdgeSizes {
            top,
            right,
            bottom,
            left,
        }
    }

    fn uniform(v: f32) -> EdgeSizes {
        edges(v, v, v, v)
    }

    fn bp(margin: EdgeSizes, padding: EdgeSizes, border: EdgeSizes) -> PackedBoxProps {
        PackedBoxProps::pack(&ResolvedBoxProps {
            margin,
            padding,
            border,
            margin_auto: MarginAuto::default(),
        })
    }

    fn bp_auto_margins(margin_auto: MarginAuto) -> PackedBoxProps {
        PackedBoxProps::pack(&ResolvedBoxProps {
            margin: uniform(0.0),
            padding: uniform(0.0),
            border: uniform(0.0),
            margin_auto,
        })
    }

    fn hot(parent: Option<usize>, dom_node_id: Option<NodeId>) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: PackedBoxProps::default(),
            dom_node_id,
            used_size: None,
            formatting_context: FormattingContext::Block {
                establishes_new_context: false,
            },
            parent,
        }
    }

    /// Hand-assembles a `LayoutTree` so the index / dangling-parent edge cases the
    /// real builder can never produce stay reachable.
    fn raw_tree(nodes: Vec<LayoutNodeHot>, child_lists: &[Vec<usize>]) -> LayoutTree {
        let n = nodes.len();
        let mut children_arena: Vec<usize> = Vec::new();
        let mut children_offsets: Vec<(u32, u32)> = Vec::with_capacity(n);
        for cl in child_lists {
            let start = u32::try_from(children_arena.len()).unwrap();
            children_arena.extend_from_slice(cl);
            children_offsets.push((start, u32::try_from(cl.len()).unwrap()));
        }
        while children_offsets.len() < n {
            children_offsets.push((0, 0));
        }
        LayoutTree {
            nodes,
            warm: vec![LayoutNodeWarm::default(); n],
            cold: vec![LayoutNodeCold::default(); n],
            root: 0,
            dom_to_layout: BTreeMap::new(),
            children_arena,
            children_offsets,
            subtree_needs_intrinsic: Vec::new(),
        }
    }

    /// `body.root > div.child`, both mirrored 1:1 into a two-node layout tree.
    fn two_level(css: &str) -> (StyledDom, LayoutTree) {
        let sd = styled(body_class("root").with_child(div_class("child")), css);
        let root = node_by_class(&sd, "root");
        let child = node_by_class(&sd, "child");
        let tree = raw_tree(
            vec![hot(None, Some(root)), hot(Some(0), Some(child))],
            &[vec![1], vec![]],
        );
        (sd, tree)
    }

    /// `body.root > div.mid > div.child`.
    fn three_level(css: &str) -> (StyledDom, LayoutTree) {
        let sd = styled(
            body_class("root").with_child(div_class("mid").with_child(div_class("child"))),
            css,
        );
        let root = node_by_class(&sd, "root");
        let mid = node_by_class(&sd, "mid");
        let child = node_by_class(&sd, "child");
        let tree = raw_tree(
            vec![
                hot(None, Some(root)),
                hot(Some(0), Some(mid)),
                hot(Some(1), Some(child)),
            ],
            &[vec![1], vec![2], vec![]],
        );
        (sd, tree)
    }

    fn positions(list: &[(f32, f32)]) -> PositionVec {
        list.iter()
            .map(|&(x, y)| LogicalPosition::new(x, y))
            .collect()
    }

    // ==================================================================
    // get_position_type (other / no-panic smoke + invariants)
    // ==================================================================

    #[test]
    fn get_position_type_none_dom_id_is_static() {
        let (sd, _tree) = two_level("");
        assert_eq!(get_position_type(&sd, None), LayoutPosition::Static);
    }

    #[test]
    fn get_position_type_unstyled_node_is_static() {
        let (sd, _tree) = two_level("");
        let child = node_by_class(&sd, "child");
        assert_eq!(get_position_type(&sd, Some(child)), LayoutPosition::Static);
    }

    #[test]
    fn get_position_type_reads_every_keyword() {
        let sd = styled(
            body_class("root")
                .with_child(div_class("st"))
                .with_child(div_class("rel"))
                .with_child(div_class("abs"))
                .with_child(div_class("fix"))
                .with_child(div_class("sticky")),
            ".st { position: static; } .rel { position: relative; } \
             .abs { position: absolute; } .fix { position: fixed; } \
             .sticky { position: sticky; }",
        );
        for (class, expected) in [
            ("st", LayoutPosition::Static),
            ("rel", LayoutPosition::Relative),
            ("abs", LayoutPosition::Absolute),
            ("fix", LayoutPosition::Fixed),
            ("sticky", LayoutPosition::Sticky),
        ] {
            let id = node_by_class(&sd, class);
            assert_eq!(get_position_type(&sd, Some(id)), expected, "class {class}");
        }
    }

    #[test]
    fn get_position_type_garbage_value_falls_back_to_static() {
        // An unparseable declaration must not leak a bogus enum — it is dropped
        // by the parser, so the cascade yields the initial value.
        let (sd, _tree) = two_level(".child { position: rubbish-42; }");
        let child = node_by_class(&sd, "child");
        assert_eq!(get_position_type(&sd, Some(child)), LayoutPosition::Static);
    }

    #[test]
    fn get_position_type_is_pure_and_stable_across_calls() {
        let (sd, _tree) = two_level(".child { position: sticky; }");
        let child = node_by_class(&sd, "child");
        let a = get_position_type(&sd, Some(child));
        let b = get_position_type(&sd, Some(child));
        assert_eq!(a, b);
        assert_eq!(a, LayoutPosition::Sticky);
        // The invariant the whole positioning pass leans on.
        assert!(a.is_positioned());
    }

    // ==================================================================
    // resolve_position_offsets (numeric)
    // ==================================================================

    #[test]
    fn resolve_position_offsets_none_dom_id_is_all_none() {
        let (sd, _tree) = two_level(".child { top: 10px; }");
        let o = resolve_position_offsets(
            &sd,
            None,
            LogicalSize::new(100.0, 100.0),
            LogicalSize::new(800.0, 600.0),
        );
        assert!(o.top.is_none() && o.right.is_none() && o.bottom.is_none() && o.left.is_none());
    }

    #[test]
    fn resolve_position_offsets_unset_insets_are_none_not_zero() {
        // `auto` must stay distinguishable from `0px` — the entire abspos
        // constraint solver branches on it.
        let (sd, _tree) = two_level("");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(100.0, 100.0),
            LogicalSize::new(800.0, 600.0),
        );
        assert!(o.top.is_none() && o.right.is_none() && o.bottom.is_none() && o.left.is_none());
    }

    #[test]
    fn resolve_position_offsets_zero_px_is_some_zero() {
        let (sd, _tree) = two_level(".child { top: 0px; left: 0px; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(0.0, 0.0),
        );
        assert_eq!(o.top, Some(0.0));
        assert_eq!(o.left, Some(0.0));
        assert!(o.right.is_none() && o.bottom.is_none());
    }

    #[test]
    fn resolve_position_offsets_px_values_round_trip() {
        let (sd, _tree) =
            two_level(".child { top: 11px; right: 22px; bottom: 33px; left: 44px; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(200.0, 100.0),
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(o.top, Some(11.0));
        assert_eq!(o.right, Some(22.0));
        assert_eq!(o.bottom, Some(33.0));
        assert_eq!(o.left, Some(44.0));
    }

    #[test]
    fn resolve_position_offsets_percent_uses_the_correct_axis() {
        // +spec:containing-block:d4b3b9 — top/bottom resolve against CB height,
        // left/right against CB width. Swapping the axes is the classic bug here.
        let (sd, _tree) =
            two_level(".child { top: 50%; bottom: 25%; left: 50%; right: 10%; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(400.0, 200.0),
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(o.top, Some(100.0), "50% of CB height 200");
        assert_eq!(o.bottom, Some(50.0), "25% of CB height 200");
        assert_eq!(o.left, Some(200.0), "50% of CB width 400");
        assert_eq!(o.right, Some(40.0), "10% of CB width 400");
    }

    #[test]
    fn resolve_position_offsets_percent_of_zero_containing_block_is_zero() {
        let (sd, _tree) = two_level(".child { top: 75%; left: 75%; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(o.top, Some(0.0));
        assert_eq!(o.left, Some(0.0));
    }

    #[test]
    fn resolve_position_offsets_negative_values_stay_negative() {
        let (sd, _tree) = two_level(".child { top: -40px; left: -25%; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(400.0, 200.0),
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(o.top, Some(-40.0));
        assert_eq!(o.left, Some(-100.0), "-25% of CB width 400");
    }

    #[test]
    fn resolve_position_offsets_em_uses_element_font_size_rem_uses_root() {
        let sd = styled(
            body_class("root").with_child(div_class("child")),
            ".root { font-size: 10px; } .child { font-size: 20px; top: 2em; left: 3rem; }",
        );
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(400.0, 200.0),
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(o.top, Some(40.0), "2em of the element's own 20px font");
        assert_eq!(o.left, Some(30.0), "3rem of the 10px root font");
    }

    #[test]
    fn resolve_position_offsets_viewport_units_use_the_viewport_not_the_containing_block() {
        let (sd, _tree) = two_level(".child { top: 10vh; left: 10vw; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(50.0, 50.0), // deliberately not the viewport
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(o.top, Some(60.0), "10vh of a 600px viewport");
        assert_eq!(o.left, Some(80.0), "10vw of an 800px viewport");
    }

    #[test]
    fn resolve_position_offsets_huge_px_bypasses_the_i16_compact_cache_intact() {
        // The compact cache encodes insets as i16 ×10 (±3276.7px) and emits a
        // sentinel outside that range. The sentinel MUST fall through to the slow
        // cascade path with the value intact — silently saturating to 3276.7px
        // (or wrapping to a negative!) would be the nasty failure here.
        let (sd, _tree) = two_level(".child { top: 100000px; left: -100000px; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(400.0, 200.0),
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(o.top, Some(100_000.0));
        assert_eq!(o.left, Some(-100_000.0));
    }

    #[test]
    fn resolve_position_offsets_around_the_i16_cache_boundary_agree_within_a_tenth_px() {
        // 3276.3px is the largest encodable value; 3276.4px trips the sentinel and
        // takes the slow path. Both paths must land on the authored value.
        let (sd, _tree) = two_level(".child { top: 3276.3px; bottom: 3276.4px; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(400.0, 200.0),
            LogicalSize::new(800.0, 600.0),
        );
        let top = o.top.expect("top is set");
        let bottom = o.bottom.expect("bottom is set");
        assert!(close(top, 3276.3, 0.1), "top was {top}");
        assert!(close(bottom, 3276.4, 0.1), "bottom was {bottom}");
    }

    #[test]
    fn resolve_position_offsets_sub_tenth_px_precision_loss_is_bounded() {
        // The i16 ×10 cache quantises to 0.1px. That is allowed — but it must not
        // drift further than that.
        let (sd, _tree) = two_level(".child { top: 10.567px; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(400.0, 200.0),
            LogicalSize::new(800.0, 600.0),
        );
        let top = o.top.expect("top is set");
        assert!(close(top, 10.567, 0.05), "top was {top}");
    }

    #[test]
    fn resolve_position_offsets_nan_containing_block_yields_nan_not_a_panic() {
        let (sd, _tree) = two_level(".child { top: 50%; left: 50%; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(f32::NAN, f32::NAN),
            LogicalSize::new(800.0, 600.0),
        );
        assert!(o.top.expect("top is set").is_nan());
        assert!(o.left.expect("left is set").is_nan());
    }

    #[test]
    fn resolve_position_offsets_infinite_containing_block_yields_infinity_not_a_panic() {
        let (sd, _tree) = two_level(".child { top: 50%; left: 50%; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(f32::INFINITY, f32::INFINITY),
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(o.top, Some(f32::INFINITY));
        assert_eq!(o.left, Some(f32::INFINITY));
    }

    #[test]
    fn resolve_position_offsets_at_f32_max_containing_block_does_not_panic() {
        let (sd, _tree) = two_level(".child { top: 100%; left: 100%; }");
        let child = node_by_class(&sd, "child");
        let o = resolve_position_offsets(
            &sd,
            Some(child),
            LogicalSize::new(f32::MAX, f32::MAX),
            LogicalSize::new(f32::MAX, f32::MAX),
        );
        // 100% of MAX is MAX (the normalized 1.0 multiply is exact).
        assert_eq!(o.top, Some(f32::MAX));
        assert_eq!(o.left, Some(f32::MAX));
    }

    // ==================================================================
    // find_absolute_containing_block_rect (numeric)
    // ==================================================================

    #[test]
    fn find_absolute_cb_rect_root_without_parent_is_the_viewport() {
        let (sd, tree) = two_level(".root { position: relative; }");
        let pos = positions(&[(0.0, 0.0), (0.0, 0.0)]);
        let got = find_absolute_containing_block_rect(&tree, 0, &sd, &pos, viewport())
            .expect("root resolves to the initial CB");
        assert_eq!(got, viewport());
    }

    #[test]
    fn find_absolute_cb_rect_out_of_range_index_is_the_viewport_not_a_panic() {
        let (sd, tree) = two_level(".root { position: relative; }");
        let pos = positions(&[(0.0, 0.0), (0.0, 0.0)]);
        let got = find_absolute_containing_block_rect(&tree, 9_999, &sd, &pos, viewport())
            .expect("an out-of-range index falls back to the initial CB");
        assert_eq!(got, viewport());
    }

    #[test]
    fn find_absolute_cb_rect_dangling_parent_index_is_an_error_not_a_panic() {
        let (sd, mut tree) = two_level(".root { position: relative; }");
        tree.nodes[1].parent = Some(9_999); // corrupt the tree
        let pos = positions(&[(0.0, 0.0), (0.0, 0.0)]);
        let got = find_absolute_containing_block_rect(&tree, 1, &sd, &pos, viewport());
        assert!(matches!(got, Err(LayoutError::InvalidTree)));
    }

    #[test]
    fn find_absolute_cb_rect_static_ancestors_fall_back_to_the_viewport() {
        let (sd, mut tree) = three_level("");
        tree.nodes[0].used_size = Some(LogicalSize::new(400.0, 300.0));
        tree.nodes[1].used_size = Some(LogicalSize::new(200.0, 100.0));
        let pos = positions(&[(0.0, 0.0), (10.0, 10.0), (20.0, 20.0)]);
        let got = find_absolute_containing_block_rect(&tree, 2, &sd, &pos, viewport())
            .expect("no positioned ancestor → initial CB");
        assert_eq!(got, viewport());
    }

    #[test]
    fn find_absolute_cb_rect_is_the_padding_box_of_the_positioned_ancestor() {
        // CSS 2.1 §10.1: padding box, i.e. margin-box origin + border, size - borders.
        let (sd, mut tree) = two_level(".root { position: relative; }");
        tree.nodes[0].used_size = Some(LogicalSize::new(400.0, 300.0));
        tree.nodes[0].box_props = bp(uniform(0.0), uniform(5.0), uniform(10.0));
        let pos = positions(&[(20.0, 30.0), (0.0, 0.0)]);
        let got = find_absolute_containing_block_rect(&tree, 1, &sd, &pos, viewport())
            .expect("relative parent is the CB");
        assert_eq!(got.origin, LogicalPosition::new(30.0, 40.0));
        assert_eq!(got.size, LogicalSize::new(380.0, 280.0));
    }

    #[test]
    fn find_absolute_cb_rect_accepts_every_positioned_ancestor_kind() {
        for keyword in ["relative", "absolute", "fixed", "sticky"] {
            let css = format!(".root {{ position: {keyword}; }}");
            let (sd, mut tree) = two_level(&css);
            tree.nodes[0].used_size = Some(LogicalSize::new(100.0, 100.0));
            let pos = positions(&[(5.0, 5.0), (0.0, 0.0)]);
            let got = find_absolute_containing_block_rect(&tree, 1, &sd, &pos, viewport())
                .expect("positioned ancestor resolves");
            assert_eq!(
                got,
                LogicalRect::new(
                    LogicalPosition::new(5.0, 5.0),
                    LogicalSize::new(100.0, 100.0)
                ),
                "position: {keyword}"
            );
        }
    }

    #[test]
    fn find_absolute_cb_rect_picks_the_nearest_positioned_ancestor() {
        let (sd, mut tree) = three_level(".root { position: relative; } .mid { position: absolute; }");
        tree.nodes[0].used_size = Some(LogicalSize::new(400.0, 300.0));
        tree.nodes[1].used_size = Some(LogicalSize::new(200.0, 100.0));
        let pos = positions(&[(0.0, 0.0), (50.0, 60.0), (0.0, 0.0)]);
        let got = find_absolute_containing_block_rect(&tree, 2, &sd, &pos, viewport())
            .expect("nearest positioned ancestor");
        assert_eq!(got.origin, LogicalPosition::new(50.0, 60.0), "mid, not root");
        assert_eq!(got.size, LogicalSize::new(200.0, 100.0));
    }

    #[test]
    fn find_absolute_cb_rect_saturating_borders_clamp_the_padding_box_to_zero() {
        // PackedBoxProps saturates each edge at 3276.7px. Two of those exceed a
        // 100px border box — the padding box must clamp to 0, never go negative.
        let (sd, mut tree) = two_level(".root { position: relative; }");
        tree.nodes[0].used_size = Some(LogicalSize::new(100.0, 100.0));
        tree.nodes[0].box_props = bp(uniform(0.0), uniform(0.0), uniform(1e30));
        let pos = positions(&[(0.0, 0.0), (0.0, 0.0)]);
        let got = find_absolute_containing_block_rect(&tree, 1, &sd, &pos, viewport())
            .expect("saturated borders still resolve");
        assert_eq!(got.size, LogicalSize::new(0.0, 0.0));
        assert!(got.size.width >= 0.0 && got.size.height >= 0.0);
        assert!(got.origin.x.is_finite() && got.origin.y.is_finite());
    }

    #[test]
    fn find_absolute_cb_rect_unsized_ancestor_is_a_zero_sized_padding_box() {
        let (sd, tree) = two_level(".root { position: relative; }"); // used_size stays None
        let pos = positions(&[(7.0, 9.0), (0.0, 0.0)]);
        let got = find_absolute_containing_block_rect(&tree, 1, &sd, &pos, viewport())
            .expect("an unsized ancestor still resolves");
        assert_eq!(got.origin, LogicalPosition::new(7.0, 9.0));
        assert_eq!(got.size, LogicalSize::new(0.0, 0.0));
    }

    #[test]
    fn find_absolute_cb_rect_missing_position_entry_defaults_to_the_origin() {
        let (sd, mut tree) = two_level(".root { position: relative; }");
        tree.nodes[0].used_size = Some(LogicalSize::new(100.0, 100.0));
        let pos: PositionVec = Vec::new(); // nothing laid out yet
        let got = find_absolute_containing_block_rect(&tree, 1, &sd, &pos, viewport())
            .expect("an empty position vec still resolves");
        assert_eq!(got.origin, LogicalPosition::new(0.0, 0.0));
        assert_eq!(got.size, LogicalSize::new(100.0, 100.0));
    }

    // ==================================================================
    // find_nearest_scrollport (numeric)
    // ==================================================================

    #[test]
    fn find_nearest_scrollport_without_a_scroll_ancestor_is_the_viewport() {
        let (sd, tree) = two_level("");
        let pos = positions(&[(0.0, 0.0), (0.0, 0.0)]);
        assert_eq!(
            find_nearest_scrollport(&tree, 1, &sd, &pos, viewport()),
            viewport()
        );
    }

    #[test]
    fn find_nearest_scrollport_out_of_range_index_is_the_viewport_not_a_panic() {
        let (sd, tree) = two_level(".root { overflow-y: scroll; }");
        let pos = positions(&[(0.0, 0.0), (0.0, 0.0)]);
        assert_eq!(
            find_nearest_scrollport(&tree, 9_999, &sd, &pos, viewport()),
            viewport()
        );
    }

    #[test]
    fn find_nearest_scrollport_returns_the_ancestor_content_box() {
        for css in [
            ".root { overflow-x: scroll; }",
            ".root { overflow-y: scroll; }",
            ".root { overflow-x: auto; }",
            ".root { overflow-y: auto; }",
        ] {
            let (sd, mut tree) = two_level(css);
            tree.nodes[0].used_size = Some(LogicalSize::new(200.0, 150.0));
            tree.nodes[0].box_props = bp(uniform(0.0), uniform(5.0), uniform(10.0));
            let pos = positions(&[(20.0, 30.0), (0.0, 0.0)]);
            let got = find_nearest_scrollport(&tree, 1, &sd, &pos, viewport());
            // content box = margin-box pos + border + padding, size - 2*(border+padding)
            assert_eq!(got.origin, LogicalPosition::new(35.0, 45.0), "{css}");
            assert_eq!(got.size, LogicalSize::new(170.0, 120.0), "{css}");
        }
    }

    #[test]
    fn find_nearest_scrollport_ignores_non_scrolling_overflow() {
        for css in [
            ".root { overflow-x: hidden; }",
            ".root { overflow-y: visible; }",
            ".root { overflow-x: clip; }",
        ] {
            let (sd, mut tree) = two_level(css);
            tree.nodes[0].used_size = Some(LogicalSize::new(200.0, 150.0));
            let pos = positions(&[(0.0, 0.0), (0.0, 0.0)]);
            assert_eq!(
                find_nearest_scrollport(&tree, 1, &sd, &pos, viewport()),
                viewport(),
                "{css}"
            );
        }
    }

    #[test]
    fn find_nearest_scrollport_picks_the_nearest_of_two_scroll_ancestors() {
        let (sd, mut tree) =
            three_level(".root { overflow-y: scroll; } .mid { overflow-y: scroll; }");
        tree.nodes[0].used_size = Some(LogicalSize::new(400.0, 300.0));
        tree.nodes[1].used_size = Some(LogicalSize::new(200.0, 100.0));
        let pos = positions(&[(0.0, 0.0), (11.0, 12.0), (0.0, 0.0)]);
        let got = find_nearest_scrollport(&tree, 2, &sd, &pos, viewport());
        assert_eq!(got.origin, LogicalPosition::new(11.0, 12.0), "mid, not root");
        assert_eq!(got.size, LogicalSize::new(200.0, 100.0));
    }

    #[test]
    fn find_nearest_scrollport_walks_past_anonymous_boxes() {
        // An anonymous box (dom_node_id: None) has no style — it must be skipped,
        // not treated as the end of the ancestor chain.
        let (sd, mut tree) = three_level(".root { overflow-y: scroll; }");
        tree.nodes[1].dom_node_id = None; // .mid becomes anonymous
        tree.nodes[0].used_size = Some(LogicalSize::new(400.0, 300.0));
        let pos = positions(&[(1.0, 2.0), (0.0, 0.0), (0.0, 0.0)]);
        let got = find_nearest_scrollport(&tree, 2, &sd, &pos, viewport());
        assert_eq!(got.origin, LogicalPosition::new(1.0, 2.0));
        assert_eq!(got.size, LogicalSize::new(400.0, 300.0));
    }

    #[test]
    fn find_nearest_scrollport_clamps_the_content_box_to_zero_when_padding_exceeds_the_box() {
        let (sd, mut tree) = two_level(".root { overflow-y: scroll; }");
        tree.nodes[0].used_size = Some(LogicalSize::new(10.0, 10.0));
        tree.nodes[0].box_props = bp(uniform(0.0), uniform(1e30), uniform(1e30));
        let pos = positions(&[(0.0, 0.0), (0.0, 0.0)]);
        let got = find_nearest_scrollport(&tree, 1, &sd, &pos, viewport());
        assert_eq!(got.size, LogicalSize::new(0.0, 0.0));
        assert!(got.size.width >= 0.0 && got.size.height >= 0.0);
    }

    #[test]
    fn find_nearest_scrollport_unsized_scrollport_is_zero_sized() {
        let (sd, tree) = two_level(".root { overflow-y: scroll; }"); // used_size None
        let pos: PositionVec = Vec::new();
        let got = find_nearest_scrollport(&tree, 1, &sd, &pos, viewport());
        assert_eq!(got.origin, LogicalPosition::new(0.0, 0.0));
        assert_eq!(got.size, LogicalSize::new(0.0, 0.0));
    }

    // ==================================================================
    // find_nearest_scroll_offset (numeric)
    // ==================================================================

    fn scroll_at(parent: (f32, f32), children: (f32, f32)) -> ScrollPosition {
        ScrollPosition {
            parent_rect: LogicalRect::new(
                LogicalPosition::new(parent.0, parent.1),
                LogicalSize::new(100.0, 100.0),
            ),
            children_rect: LogicalRect::new(
                LogicalPosition::new(children.0, children.1),
                LogicalSize::new(100.0, 400.0),
            ),
        }
    }

    #[test]
    fn find_nearest_scroll_offset_empty_map_is_zero() {
        let (_sd, tree) = two_level("");
        let offsets: BTreeMap<NodeId, ScrollPosition> = BTreeMap::new();
        assert_eq!(
            find_nearest_scroll_offset(&tree, 1, &offsets),
            LogicalPosition::zero()
        );
    }

    #[test]
    fn find_nearest_scroll_offset_out_of_range_index_is_zero_not_a_panic() {
        let (sd, tree) = two_level("");
        let mut offsets = BTreeMap::new();
        offsets.insert(node_by_class(&sd, "root"), scroll_at((0.0, 0.0), (0.0, -50.0)));
        assert_eq!(
            find_nearest_scroll_offset(&tree, 9_999, &offsets),
            LogicalPosition::zero()
        );
    }

    #[test]
    fn find_nearest_scroll_offset_ignores_the_nodes_own_entry() {
        // The walk starts at the PARENT — a node's own scroll offset must not
        // shift the node itself.
        let (sd, tree) = two_level("");
        let mut offsets = BTreeMap::new();
        offsets.insert(
            node_by_class(&sd, "child"),
            scroll_at((0.0, 0.0), (0.0, -50.0)),
        );
        assert_eq!(
            find_nearest_scroll_offset(&tree, 1, &offsets),
            LogicalPosition::zero()
        );
    }

    #[test]
    fn find_nearest_scroll_offset_is_children_origin_minus_parent_origin() {
        let (sd, tree) = two_level("");
        let mut offsets = BTreeMap::new();
        offsets.insert(
            node_by_class(&sd, "root"),
            scroll_at((10.0, 20.0), (-5.0, -80.0)),
        );
        assert_eq!(
            find_nearest_scroll_offset(&tree, 1, &offsets),
            LogicalPosition::new(-15.0, -100.0)
        );
    }

    #[test]
    fn find_nearest_scroll_offset_picks_the_nearest_ancestor() {
        let (sd, tree) = three_level("");
        let mut offsets = BTreeMap::new();
        offsets.insert(node_by_class(&sd, "root"), scroll_at((0.0, 0.0), (0.0, -999.0)));
        offsets.insert(node_by_class(&sd, "mid"), scroll_at((0.0, 0.0), (0.0, -7.0)));
        assert_eq!(
            find_nearest_scroll_offset(&tree, 2, &offsets),
            LogicalPosition::new(0.0, -7.0),
            "mid wins over root"
        );
    }

    #[test]
    fn find_nearest_scroll_offset_walks_past_anonymous_ancestors() {
        let (sd, mut tree) = three_level("");
        tree.nodes[1].dom_node_id = None;
        let mut offsets = BTreeMap::new();
        offsets.insert(node_by_class(&sd, "root"), scroll_at((0.0, 0.0), (0.0, -30.0)));
        assert_eq!(
            find_nearest_scroll_offset(&tree, 2, &offsets),
            LogicalPosition::new(0.0, -30.0)
        );
    }

    #[test]
    fn find_nearest_scroll_offset_at_f32_extremes_stays_deterministic() {
        let (sd, tree) = two_level("");
        let mut offsets = BTreeMap::new();
        offsets.insert(
            node_by_class(&sd, "root"),
            scroll_at((f32::MAX, f32::MAX), (f32::MIN, f32::MIN)),
        );
        let got = find_nearest_scroll_offset(&tree, 1, &offsets);
        // MIN - MAX overflows f32 → -inf. It must not be NaN (which would poison
        // every downstream sticky comparison silently).
        assert!(!got.x.is_nan() && !got.y.is_nan());
        assert_eq!(got.x, f32::NEG_INFINITY);
        assert_eq!(got.y, f32::NEG_INFINITY);
    }

    // ==================================================================
    // The three passes that need a LayoutContext (and therefore a FontManager).
    // ==================================================================
    #[cfg(all(feature = "text_layout", feature = "font_loading"))]
    mod with_ctx {
        use std::collections::HashMap;

        use azul_core::{dom::DomId, selection::TextSelection};
        use azul_css::props::basic::FontRef;

        use super::*;
        use crate::{
            font_traits::{FontManager, TextLayoutCache},
            solver3::{cache, LayoutContext},
        };

        /// Owns everything a `LayoutContext` borrows.
        struct Env {
            styled_dom: StyledDom,
            font_manager: FontManager<FontRef>,
            text_selections: BTreeMap<DomId, TextSelection>,
            counters: HashMap<(usize, String), i32>,
            image_cache: azul_core::resources::ImageCache,
            debug_messages: Option<Vec<LayoutDebugMessage>>,
        }

        impl Env {
            fn new(styled_dom: StyledDom) -> Self {
                Self {
                    styled_dom,
                    font_manager: FontManager::new(rust_fontconfig::FcFontCache::default())
                        .expect("FontManager over an empty font cache"),
                    text_selections: BTreeMap::new(),
                    counters: HashMap::new(),
                    image_cache: azul_core::resources::ImageCache::default(),
                    debug_messages: None,
                }
            }

            fn ctx(&mut self) -> LayoutContext<'_, FontRef> {
                LayoutContext {
                    scrollbar_style_cache: core::cell::RefCell::new(HashMap::new()),
                    styled_dom: &self.styled_dom,
                    font_manager: &self.font_manager,
                    text_selections: &self.text_selections,
                    debug_messages: &mut self.debug_messages,
                    counters: &mut self.counters,
                    viewport_size: LogicalSize::new(800.0, 600.0),
                    fragmentation_context: None,
                    cursor_is_visible: true,
                    cursor_locations: Vec::new(),
                    preedit_text: None,
                    dirty_text_overrides: BTreeMap::new(),
                    cache_map: cache::LayoutCacheMap::default(),
                    image_cache: &self.image_cache,
                    system_style: None,
                    get_system_time_fn: azul_core::task::GetSystemTimeCallback {
                        cb: azul_core::task::get_system_time_libstd,
                    },
                }
            }
        }

        /// `.root` = relative, 400×300 border box, 10px border + 5px padding, at (20,30).
        /// Its padding box — the CB every abspos child below resolves against — is
        /// therefore origin (30,40), size 380×280.
        fn abs_fixture(css: &str) -> (Env, LayoutTree, PositionVec) {
            let (sd, mut tree) = two_level(css);
            tree.nodes[0].used_size = Some(LogicalSize::new(400.0, 300.0));
            tree.nodes[0].box_props = bp(uniform(0.0), uniform(5.0), uniform(10.0));
            tree.nodes[1].used_size = Some(LogicalSize::new(50.0, 50.0));
            let pos = positions(&[(20.0, 30.0), (0.0, 0.0)]);
            (Env::new(sd), tree, pos)
        }

        fn run_oof(env: &mut Env, tree: &mut LayoutTree, pos: &mut PositionVec, vp: LogicalRect) {
            let mut text_cache = TextLayoutCache::default();
            let mut ctx = env.ctx();
            position_out_of_flow_elements(&mut ctx, tree, &mut text_cache, pos, vp);
        }

        // --------------------------------------------------------------
        // position_out_of_flow_elements
        // --------------------------------------------------------------

        #[test]
        fn out_of_flow_top_left_offset_from_the_ancestor_padding_box() {
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: absolute; top: 25px; left: 15px; }",
            );
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            assert_eq!(pos[1], LogicalPosition::new(45.0, 65.0));
        }

        #[test]
        fn out_of_flow_zero_insets_land_exactly_on_the_padding_box_origin() {
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } .child { position: absolute; top: 0px; left: 0px; }",
            );
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            assert_eq!(pos[1], LogicalPosition::new(30.0, 40.0));
        }

        #[test]
        fn out_of_flow_all_auto_keeps_the_static_position() {
            // +spec:positioning:aab294 — both insets auto → static position.
            let (mut env, mut tree, mut pos) =
                abs_fixture(".root { position: relative; } .child { position: absolute; }");
            pos_set(&mut pos, 1, LogicalPosition::new(7.0, 9.0));
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            assert_eq!(pos[1], LogicalPosition::new(7.0, 9.0));
        }

        #[test]
        fn out_of_flow_fixed_resolves_against_the_viewport_not_the_ancestor() {
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } .child { position: fixed; top: 25px; left: 15px; }",
            );
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            assert_eq!(pos[1], LogicalPosition::new(15.0, 25.0));
        }

        #[test]
        fn out_of_flow_over_constrained_ignores_the_end_insets_in_ltr() {
            // top/height/bottom and left/width/right all given: bottom/right lose.
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: absolute; top: 10px; bottom: 10px; left: 10px; \
                          right: 10px; width: 50px; height: 50px; }",
            );
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            assert_eq!(pos[1], LogicalPosition::new(40.0, 50.0));
        }

        #[test]
        fn out_of_flow_auto_margins_center_the_box_in_both_axes() {
            // +spec:height-calculation:5112a4 — both auto margins solve to equal values.
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: absolute; top: 0px; bottom: 0px; left: 0px; \
                          right: 0px; width: 100px; height: 100px; }",
            );
            tree.nodes[1].used_size = Some(LogicalSize::new(100.0, 100.0));
            tree.nodes[1].box_props = bp_auto_margins(MarginAuto {
                top: true,
                bottom: true,
                left: true,
                right: true,
            });
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            // CB 380×280 at (30,40): (380-100)/2 = 140, (280-100)/2 = 90.
            assert_eq!(pos[1], LogicalPosition::new(170.0, 130.0));
        }

        #[test]
        fn out_of_flow_negative_free_space_with_auto_margins_pins_to_the_start_edge_in_ltr() {
            // +spec:writing-modes:9c3b40 — negative remaining space: start margin is 0.
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: absolute; left: 0px; right: 0px; width: 500px; }",
            );
            tree.nodes[1].used_size = Some(LogicalSize::new(500.0, 50.0));
            tree.nodes[1].box_props = bp_auto_margins(MarginAuto {
                left: true,
                right: true,
                top: false,
                bottom: false,
            });
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            // remaining = 380 - 0 - 500 - 0 = -120 → each margin < 0 → pin left.
            assert_eq!(pos[1].x, 30.0);
        }

        #[test]
        fn out_of_flow_over_constrained_ignores_the_left_inset_in_rtl() {
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; direction: rtl; } \
                 .child { position: absolute; left: 10px; right: 10px; width: 50px; }",
            );
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            // RTL solves for left: 380 - 50 - 10 = 320 → 30 + 320.
            assert_eq!(pos[1].x, 350.0);
        }

        #[test]
        fn out_of_flow_auto_height_and_width_stretch_between_the_insets() {
            // +spec:intrinsic-sizing:566a43 — stretch-fit sizing on both axes.
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: absolute; top: 10px; bottom: 20px; left: 30px; right: 40px; }",
            );
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            assert_eq!(pos[1], LogicalPosition::new(60.0, 50.0));
            let used = tree.nodes[1].used_size.expect("size was resolved");
            assert_eq!(used, LogicalSize::new(310.0, 250.0));
        }

        #[test]
        fn out_of_flow_insets_larger_than_the_containing_block_clamp_the_size_to_zero() {
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: absolute; top: 500px; bottom: 500px; \
                          left: 500px; right: 500px; }",
            );
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            let used = tree.nodes[1].used_size.expect("size was resolved");
            assert_eq!(used, LogicalSize::new(0.0, 0.0), "never negative");
            assert!(pos[1].x.is_finite() && pos[1].y.is_finite());
        }

        #[test]
        fn out_of_flow_huge_insets_bypass_the_i16_cache_and_stay_finite() {
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: absolute; top: 3300px; left: 100000px; }",
            );
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            assert_eq!(pos[1], LogicalPosition::new(100_030.0, 3340.0));
            assert!(pos[1].x.is_finite() && pos[1].y.is_finite());
        }

        #[test]
        fn out_of_flow_negative_insets_move_the_box_outside_the_containing_block() {
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: absolute; top: -100px; left: -200px; }",
            );
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            assert_eq!(pos[1], LogicalPosition::new(-170.0, -60.0));
        }

        #[test]
        fn out_of_flow_nan_viewport_clamps_the_stretch_height_to_zero_and_keeps_the_position_finite()
        {
            // f32::max(NaN, 0.0) == 0.0, so the stretch-fit height degrades to 0
            // rather than propagating NaN into the display list.
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: fixed; top: 10px; bottom: 20px; }",
            );
            let nan_vp = LogicalRect::new(
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(f32::NAN, f32::NAN),
            );
            run_oof(&mut env, &mut tree, &mut pos, nan_vp);
            let used = tree.nodes[1].used_size.expect("size was resolved");
            assert_eq!(used.height, 0.0);
            assert_eq!(pos[1].y, 10.0);
            assert!(pos[1].y.is_finite());
        }

        #[test]
        fn out_of_flow_infinite_viewport_keeps_the_position_finite() {
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: fixed; top: 10px; bottom: 20px; }",
            );
            let inf_vp = LogicalRect::new(
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(f32::INFINITY, f32::INFINITY),
            );
            run_oof(&mut env, &mut tree, &mut pos, inf_vp);
            assert_eq!(pos[1].y, 10.0);
            let used = tree.nodes[1].used_size.expect("size was resolved");
            assert!(used.height.is_infinite() && used.height > 0.0);
        }

        #[test]
        fn out_of_flow_every_auto_combination_of_top_height_bottom_is_panic_free() {
            // The rustdoc claims a panic when a resolved offset is None where both
            // edges are expected. Walk all 8 auto/non-auto combinations per axis and
            // prove every `unwrap()` in the constraint solver is actually guarded.
            for top in ["", "top: 10px;"] {
                for bottom in ["", "bottom: 20px;"] {
                    for height in ["", "height: 30px;"] {
                        for left in ["", "left: 10px;"] {
                            for right in ["", "right: 20px;"] {
                                for width in ["", "width: 30px;"] {
                                    let css = format!(
                                        ".root {{ position: relative; }} \
                                         .child {{ position: absolute; {top}{bottom}{height}\
                                         {left}{right}{width} }}"
                                    );
                                    let (mut env, mut tree, mut pos) = abs_fixture(&css);
                                    run_oof(&mut env, &mut tree, &mut pos, viewport());
                                    assert!(
                                        pos[1].x.is_finite() && pos[1].y.is_finite(),
                                        "non-finite position for {css}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        #[test]
        fn out_of_flow_skips_children_of_flex_and_grid_parents() {
            // Taffy already placed those during flex/grid layout — re-positioning
            // here would double-apply the insets.
            for fc in [FormattingContext::Flex, FormattingContext::Grid] {
                let (mut env, mut tree, mut pos) = abs_fixture(
                    ".root { position: relative; } \
                     .child { position: absolute; top: 25px; left: 15px; }",
                );
                tree.nodes[0].formatting_context = fc;
                pos_set(&mut pos, 1, LogicalPosition::new(3.0, 4.0));
                run_oof(&mut env, &mut tree, &mut pos, viewport());
                assert_eq!(pos[1], LogicalPosition::new(3.0, 4.0), "{fc:?}");
            }
        }

        #[test]
        fn out_of_flow_leaves_static_and_relative_nodes_alone() {
            for keyword in ["static", "relative", "sticky"] {
                let css = format!(
                    ".root {{ position: relative; }} \
                     .child {{ position: {keyword}; top: 25px; left: 15px; }}"
                );
                let (mut env, mut tree, mut pos) = abs_fixture(&css);
                pos_set(&mut pos, 1, LogicalPosition::new(3.0, 4.0));
                run_oof(&mut env, &mut tree, &mut pos, viewport());
                assert_eq!(pos[1], LogicalPosition::new(3.0, 4.0), "{keyword}");
            }
        }

        #[test]
        fn out_of_flow_short_position_vec_grows_instead_of_panicking() {
            let (mut env, mut tree, _pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: absolute; top: 25px; left: 15px; }",
            );
            let mut pos: PositionVec = Vec::new(); // nothing laid out yet
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            assert_eq!(pos.len(), 2, "pos_set grew the vec");
            // The CB origin now comes from a default (0,0) ancestor position.
            assert_eq!(pos[1], LogicalPosition::new(25.0, 35.0));
        }

        #[test]
        fn out_of_flow_unsized_node_is_sized_on_the_fly_without_panicking() {
            let (mut env, mut tree, mut pos) = abs_fixture(
                ".root { position: relative; } \
                 .child { position: absolute; top: 10px; left: 10px; }",
            );
            tree.nodes[1].used_size = None; // never sized by the main pass
            run_oof(&mut env, &mut tree, &mut pos, viewport());
            assert!(pos[1].x.is_finite() && pos[1].y.is_finite());
        }

        // --------------------------------------------------------------
        // adjust_relative_positions
        // --------------------------------------------------------------

        /// `.root` = 200×100 border box with 10px padding → 180×80 content box,
        /// which is the CB percentages resolve against for the relative child.
        fn rel_fixture(css: &str) -> (Env, LayoutTree, PositionVec) {
            let (sd, mut tree) = two_level(css);
            tree.nodes[0].used_size = Some(LogicalSize::new(200.0, 100.0));
            tree.nodes[0].box_props = bp(uniform(0.0), uniform(10.0), uniform(0.0));
            tree.nodes[1].used_size = Some(LogicalSize::new(50.0, 20.0));
            let pos = positions(&[(0.0, 0.0), (100.0, 100.0)]);
            (Env::new(sd), tree, pos)
        }

        fn run_rel(env: &mut Env, tree: &LayoutTree, pos: &mut PositionVec) {
            let mut ctx = env.ctx();
            adjust_relative_positions(&mut ctx, tree, pos, viewport());
        }

        #[test]
        fn relative_px_offsets_shift_from_the_static_position() {
            let (mut env, tree, mut pos) =
                rel_fixture(".child { position: relative; top: 10px; left: 5px; }");
            run_rel(&mut env, &tree, &mut pos);
            assert_eq!(pos[1], LogicalPosition::new(105.0, 110.0));
        }

        #[test]
        fn relative_percentages_resolve_against_the_parent_content_box() {
            let (mut env, tree, mut pos) =
                rel_fixture(".child { position: relative; top: 50%; left: 50%; }");
            run_rel(&mut env, &tree, &mut pos);
            // content box is 180×80 → +90 x, +40 y.
            assert_eq!(pos[1], LogicalPosition::new(190.0, 140.0));
        }

        #[test]
        fn relative_top_wins_over_bottom() {
            // +spec:positioning:e3727e — neither auto → bottom is ignored.
            let (mut env, tree, mut pos) =
                rel_fixture(".child { position: relative; top: 10px; bottom: 30px; }");
            run_rel(&mut env, &tree, &mut pos);
            assert_eq!(pos[1].y, 110.0);
        }

        #[test]
        fn relative_bottom_alone_is_the_negation_of_top() {
            let (mut env, tree, mut pos) =
                rel_fixture(".child { position: relative; bottom: 30px; }");
            run_rel(&mut env, &tree, &mut pos);
            assert_eq!(pos[1].y, 70.0);
        }

        #[test]
        fn relative_right_alone_is_the_negation_of_left() {
            // +spec:overflow:fb426c — left auto → used value is minus right.
            let (mut env, tree, mut pos) =
                rel_fixture(".child { position: relative; right: 20px; }");
            run_rel(&mut env, &tree, &mut pos);
            assert_eq!(pos[1].x, 80.0);
        }

        #[test]
        fn relative_left_wins_in_ltr_and_right_wins_in_rtl() {
            // +spec:containing-block:6d4fb1 — direction of the CONTAINING BLOCK decides.
            let (mut env, tree, mut pos) =
                rel_fixture(".child { position: relative; left: 5px; right: 20px; }");
            run_rel(&mut env, &tree, &mut pos);
            assert_eq!(pos[1].x, 105.0, "ltr: left wins");

            let (mut env, tree, mut pos) = rel_fixture(
                ".root { direction: rtl; } \
                 .child { position: relative; left: 5px; right: 20px; }",
            );
            run_rel(&mut env, &tree, &mut pos);
            assert_eq!(pos[1].x, 80.0, "rtl: right wins → -20");
        }

        #[test]
        fn relative_zero_offsets_are_a_no_op() {
            let (mut env, tree, mut pos) =
                rel_fixture(".child { position: relative; top: 0px; left: 0px; }");
            run_rel(&mut env, &tree, &mut pos);
            assert_eq!(pos[1], LogicalPosition::new(100.0, 100.0));
        }

        #[test]
        fn relative_leaves_static_absolute_and_fixed_nodes_untouched() {
            for keyword in ["static", "absolute", "fixed"] {
                let css =
                    format!(".child {{ position: {keyword}; top: 10px; left: 5px; }}");
                let (mut env, tree, mut pos) = rel_fixture(&css);
                run_rel(&mut env, &tree, &mut pos);
                assert_eq!(pos[1], LogicalPosition::new(100.0, 100.0), "{keyword}");
            }
        }

        #[test]
        fn relative_also_offsets_sticky_boxes() {
            // Sticky deliberately shares the relative path (the pre-scroll offset);
            // adjust_sticky_positions then clamps it. Pinning this so the two passes
            // can't silently start disagreeing.
            let (mut env, tree, mut pos) =
                rel_fixture(".child { position: relative; top: 10px; }");
            run_rel(&mut env, &tree, &mut pos);
            let relative_y = pos[1].y;

            let (mut env, tree, mut pos) =
                rel_fixture(".child { position: sticky; top: 10px; }");
            run_rel(&mut env, &tree, &mut pos);
            assert_eq!(pos[1].y, relative_y);
        }

        #[test]
        fn relative_is_undefined_for_table_cells_and_captions_so_they_are_skipped() {
            for display in ["table-cell", "table-caption", "table-column"] {
                let css = format!(
                    ".child {{ position: relative; display: {display}; top: 10px; left: 5px; }}"
                );
                let (mut env, tree, mut pos) = rel_fixture(&css);
                run_rel(&mut env, &tree, &mut pos);
                assert_eq!(pos[1], LogicalPosition::new(100.0, 100.0), "{display}");
            }
        }

        #[test]
        fn relative_table_rows_drag_their_whole_subtree() {
            // +spec:table-layout:ec2600 — the shift affects all contents of the row.
            let (sd, mut tree) = three_level(
                ".mid { position: relative; display: table-row; top: 10px; left: 5px; } \
                 .child { display: table-cell; }",
            );
            tree.nodes[0].used_size = Some(LogicalSize::new(200.0, 100.0));
            tree.nodes[1].used_size = Some(LogicalSize::new(200.0, 50.0));
            tree.nodes[2].used_size = Some(LogicalSize::new(100.0, 50.0));
            let mut pos = positions(&[(0.0, 0.0), (10.0, 20.0), (10.0, 20.0)]);
            let mut env = Env::new(sd);
            run_rel(&mut env, &tree, &mut pos);
            assert_eq!(pos[1], LogicalPosition::new(15.0, 30.0), "the row itself");
            assert_eq!(pos[2], LogicalPosition::new(15.0, 30.0), "the cell follows");
        }

        #[test]
        fn relative_short_position_vec_is_skipped_not_panicked_on() {
            let (mut env, tree, _pos) =
                rel_fixture(".child { position: relative; top: 10px; left: 5px; }");
            let mut pos: PositionVec = Vec::new();
            run_rel(&mut env, &tree, &mut pos);
            assert!(pos.is_empty(), "nothing to shift, nothing added");
        }

        #[test]
        fn relative_huge_and_negative_offsets_stay_finite() {
            let (mut env, tree, mut pos) =
                rel_fixture(".child { position: relative; top: 100000px; left: -100000px; }");
            run_rel(&mut env, &tree, &mut pos);
            assert_eq!(pos[1], LogicalPosition::new(-99_900.0, 100_100.0));
            assert!(pos[1].x.is_finite() && pos[1].y.is_finite());
        }

        #[test]
        fn relative_unset_sentinel_position_is_not_silently_shifted_into_a_real_one() {
            // POSITION_UNSET is f32::MIN. Adding a finite delta to it must stay
            // absurdly negative (it must NOT round into a plausible coordinate) —
            // a caller can still detect the node was never laid out.
            let (mut env, tree, mut pos) =
                rel_fixture(".child { position: relative; top: 10px; left: 5px; }");
            pos[1] = POSITION_UNSET;
            run_rel(&mut env, &tree, &mut pos);
            assert!(pos[1].x < -1e30 && pos[1].y < -1e30);
        }

        // --------------------------------------------------------------
        // adjust_sticky_positions
        // --------------------------------------------------------------

        /// `.root` = a 200×200 scrollport at (0,0); `.child` = 50×20 sticky box at (0,0).
        fn sticky_fixture(css: &str) -> (Env, LayoutTree, PositionVec) {
            let (sd, mut tree) = two_level(css);
            tree.nodes[0].used_size = Some(LogicalSize::new(200.0, 200.0));
            tree.nodes[1].used_size = Some(LogicalSize::new(50.0, 20.0));
            let pos = positions(&[(0.0, 0.0), (0.0, 0.0)]);
            (Env::new(sd), tree, pos)
        }

        fn run_sticky(
            env: &mut Env,
            tree: &LayoutTree,
            pos: &mut PositionVec,
            offsets: &BTreeMap<NodeId, ScrollPosition>,
        ) {
            let mut ctx = env.ctx();
            adjust_sticky_positions(&mut ctx, tree, pos, offsets, viewport());
        }

        #[test]
        fn sticky_top_inset_pins_the_box_to_the_scrollport_edge() {
            let (mut env, tree, mut pos) = sticky_fixture(
                ".root { overflow-y: scroll; } .child { position: sticky; top: 10px; }",
            );
            run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
            assert_eq!(pos[1], LogicalPosition::new(0.0, 10.0));
        }

        #[test]
        fn sticky_without_insets_does_not_move() {
            let (mut env, tree, mut pos) =
                sticky_fixture(".root { overflow-y: scroll; } .child { position: sticky; }");
            run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
            assert_eq!(pos[1], LogicalPosition::new(0.0, 0.0));
        }

        #[test]
        fn sticky_ignores_non_sticky_positions() {
            for keyword in ["static", "relative", "absolute", "fixed"] {
                let css = format!(
                    ".root {{ overflow-y: scroll; }} \
                     .child {{ position: {keyword}; top: 10px; }}"
                );
                let (mut env, tree, mut pos) = sticky_fixture(&css);
                run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
                assert_eq!(pos[1], LogicalPosition::new(0.0, 0.0), "{keyword}");
            }
        }

        #[test]
        fn sticky_edge_moves_with_the_scroll_offset_of_the_nearest_container() {
            let (mut env, tree, mut pos) = sticky_fixture(
                ".root { overflow-y: scroll; } .child { position: sticky; top: 10px; }",
            );
            let root = node_by_class(&env.styled_dom, "root");
            let mut offsets = BTreeMap::new();
            offsets.insert(root, scroll_at((0.0, 0.0), (0.0, 50.0)));
            run_sticky(&mut env, &tree, &mut pos, &offsets);
            // sticky edge = scrollport.y (0) + scroll (50) + inset (10).
            assert_eq!(pos[1].y, 60.0);
        }

        #[test]
        fn sticky_percentage_inset_resolves_against_the_scrollport() {
            let (mut env, tree, mut pos) = sticky_fixture(
                ".root { overflow-y: scroll; } .child { position: sticky; top: 10%; }",
            );
            run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
            assert_eq!(pos[1].y, 20.0, "10% of the 200px scrollport");
        }

        #[test]
        fn sticky_bottom_inset_pulls_the_box_back_up_into_the_scrollport() {
            let (mut env, tree, mut pos) = sticky_fixture(
                ".root { overflow-y: scroll; } .child { position: sticky; bottom: 10px; }",
            );
            pos_set(&mut pos, 1, LogicalPosition::new(0.0, 250.0));
            run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
            // bottom edge must sit at 200 - 10 = 190 → top = 190 - 20.
            assert_eq!(pos[1].y, 170.0);
        }

        #[test]
        fn sticky_shift_is_clamped_by_the_containing_block() {
            // +spec:box-model:af9af8 — the margin box must stay inside the CB, even
            // when the scrollport would let the box travel further.
            let (sd, mut tree) = three_level(
                ".root { overflow-y: scroll; } .child { position: sticky; top: 10px; }",
            );
            tree.nodes[0].used_size = Some(LogicalSize::new(200.0, 200.0));
            tree.nodes[1].used_size = Some(LogicalSize::new(200.0, 25.0)); // short CB
            tree.nodes[2].used_size = Some(LogicalSize::new(50.0, 20.0));
            let mut pos = positions(&[(0.0, 0.0), (0.0, 0.0), (0.0, 0.0)]);
            let mut env = Env::new(sd);
            run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
            // Unclamped the shift would be 10 (bottom = 30 > CB bottom 25) → 5.
            assert_eq!(pos[2].y, 5.0);
        }

        #[test]
        fn sticky_huge_inset_clamps_to_the_containing_block_instead_of_flying_away() {
            let (mut env, tree, mut pos) = sticky_fixture(
                ".root { overflow-y: scroll; } .child { position: sticky; top: 100000px; }",
            );
            run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
            // The margin box is pushed back until its bottom sits on the CB bottom
            // (200) → top = 200 - 20 = 180.
            assert_eq!(pos[1].y, 180.0);
            assert!(pos[1].y.is_finite());
        }

        #[test]
        fn sticky_negative_inset_is_deterministic_and_finite() {
            let (mut env, tree, mut pos) = sticky_fixture(
                ".root { overflow-y: scroll; } .child { position: sticky; top: -50px; }",
            );
            run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
            // sticky edge = -50, border top = 0, already past it → no shift.
            assert_eq!(pos[1], LogicalPosition::new(0.0, 0.0));
        }

        #[test]
        fn sticky_without_a_scroll_ancestor_falls_back_to_the_viewport() {
            let (mut env, tree, mut pos) =
                sticky_fixture(".child { position: sticky; top: 10px; }"); // .root does not scroll
            run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
            // Scrollport = viewport (0,0,800×600); CB = the parent's 200×200 content
            // box, which comfortably contains the 10px shift.
            assert_eq!(pos[1].y, 10.0);
        }

        #[test]
        fn sticky_left_and_right_insets_shift_the_inline_axis() {
            let (mut env, tree, mut pos) = sticky_fixture(
                ".root { overflow-x: scroll; } .child { position: sticky; left: 15px; }",
            );
            run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
            assert_eq!(pos[1].x, 15.0);

            let (mut env, tree, mut pos) = sticky_fixture(
                ".root { overflow-x: scroll; } .child { position: sticky; right: 10px; }",
            );
            pos_set(&mut pos, 1, LogicalPosition::new(300.0, 0.0));
            run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
            // right edge pinned at 200 - 10 = 190 → x = 190 - 50.
            assert_eq!(pos[1].x, 140.0);
        }

        #[test]
        fn sticky_short_position_vec_is_skipped_not_panicked_on() {
            let (mut env, tree, _pos) = sticky_fixture(
                ".root { overflow-y: scroll; } .child { position: sticky; top: 10px; }",
            );
            let mut pos: PositionVec = Vec::new();
            run_sticky(&mut env, &tree, &mut pos, &BTreeMap::new());
            assert!(pos.is_empty());
        }

        #[test]
        fn sticky_nan_scroll_offset_never_panics() {
            let (mut env, tree, mut pos) = sticky_fixture(
                ".root { overflow-y: scroll; } .child { position: sticky; top: 10px; }",
            );
            let root = node_by_class(&env.styled_dom, "root");
            let mut offsets = BTreeMap::new();
            offsets.insert(root, scroll_at((f32::NAN, f32::NAN), (f32::NAN, f32::NAN)));
            run_sticky(&mut env, &tree, &mut pos, &offsets);
            // NaN comparisons are all false → no shift is ever applied.
            assert_eq!(pos[1], LogicalPosition::new(0.0, 0.0));
        }
    }
}
