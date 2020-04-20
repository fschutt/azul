#![allow(dead_code)]
#![allow(unused_assignments)]
#![allow(unused_variables)]

use std::{fmt, collections::BTreeMap};
use crate::{
    RectContent,
    RectContent::{Image, Text},
    anon::{AnonDom, AnonNode::{InlineNode, BlockNode, AnonStyle}},
    style::{Style, Overflow as StyleOverflow, Display, Dimension, BoxSizing},
    number::{Number, Number::{Defined, Undefined}, MinMax, OrElse},
};
use azul_core::{
    traits::GetTextLayout,
    id_tree::{NodeDataContainer, NodeHierarchy, NodeDepths, NodeId},
    ui_solver::{
        PositionedRectangle, InlineTextLayout, PositionInfo,
        ResolvedTextLayoutOptions, ResolvedOffsets
    },
};
use azul_css::{LayoutSize, LayoutPoint, LayoutRect, Overflow};

pub(crate) fn compute<T: GetTextLayout>(
    root_size: LayoutSize,
    anon_dom: &AnonDom,
    rect_contents: &mut BTreeMap<NodeId, RectContent<T>>,
) -> NodeDataContainer<PositionedRectangle> {

    let anon_dom_depths = anon_dom.anon_node_hierarchy.get_parents_sorted_by_depth();

    let mut positioned_rects = NodeDataContainer::new(vec![PositionedRectangle {
        size: LayoutSize::zero(),
        position: PositionInfo::Static { x_offset: 0.0, y_offset: 0.0 },
        padding: ResolvedOffsets::zero(),
        margin: ResolvedOffsets::zero(),
        border_widths: ResolvedOffsets::zero(),
        resolved_text_layout_options: None,
        overflow: Overflow::Scroll,
    }; anon_dom.anon_node_hierarchy.len()]);

    let mut resolved_text_layout_options = BTreeMap::new();

    solve_widths(
        root_size.width,
        anon_dom,
        &anon_dom_depths,
        &mut positioned_rects,
        rect_contents,
        &mut resolved_text_layout_options,
    );

    solve_heights(
        root_size,
        anon_dom,
        &anon_dom_depths,
        rect_contents,
        &mut positioned_rects,
        &resolved_text_layout_options,
    );

    position_items(
        root_size,
        anon_dom,
        &anon_dom_depths,
        &mut positioned_rects,
    );

    NodeDataContainer::new(
        anon_dom.original_node_id_mapping
        .iter()
        .map(|(_, anon_node_id)| {
            let mut pos_rect = positioned_rects[*anon_node_id].clone();
            if let Some((rtlo, inline_text)) = resolved_text_layout_options.get(anon_node_id).cloned() {
                let bounds = inline_text.get_bounds();
                pos_rect.resolved_text_layout_options = Some((rtlo, inline_text, bounds));
            }
            pos_rect
        })
        .collect()
    )
}

fn solve_widths<T: GetTextLayout>(
    root_width: f32,
    anon_dom: &AnonDom,
    anon_dom_depths: &NodeDepths,
    positioned_rects: &mut NodeDataContainer<PositionedRectangle>,
    rect_contents: &mut BTreeMap<NodeId, RectContent<T>>,
    resolved_text_layout_options: &mut BTreeMap<NodeId, (ResolvedTextLayoutOptions, InlineTextLayout)>,
) {

    debug_assert!(anon_dom.anon_node_hierarchy.len() == positioned_rects.len());

    let mut last_trailing = None;

    macro_rules! calc_block_width {($id:expr, $parent_content_size:expr) => ({
        let id: NodeId = $id;
        if $parent_content_size > 0.0 {
            let block_width = match &anon_dom.anon_node_data[id] {
                BlockNode(ref style) => calculate_block_width(style, $parent_content_size),
                InlineNode(ref style) => {

                    let original_node_id = &anon_dom.reverse_node_id_mapping.get(&id).unwrap();
                    let parent_overflow_x = anon_dom.anon_node_hierarchy[id].parent.as_ref().map(|parent_id| {
                        anon_dom.anon_node_data[*parent_id].get_overflow_x()
                    }).unwrap_or(StyleOverflow::Auto);

                    let w = calculate_inline_width(
                        id,
                        last_trailing,
                        rect_contents.get_mut(original_node_id),
                        style,
                        $parent_content_size,
                        parent_overflow_x,
                        resolved_text_layout_options,
                    );

                    last_trailing = w.trailing;

                    BlockWidth {
                        width: w.width,
                        border_width: w.border_width,
                        margin: w.margin,
                        padding: w.padding,
                    }
                },
                AnonStyle => {
                    // set padding, margin, border to zero
                    BlockWidth { width: $parent_content_size, .. Default::default() }
                }
            };

            apply_block_width(block_width, &mut positioned_rects[$id]);
        }
    })}

    calc_block_width!(NodeId::ZERO, root_width);

    for (_depth, parent_node_id) in anon_dom_depths {
        let parent_content_size = positioned_rects[*parent_node_id].size.width;
        for child_id in parent_node_id.children(&anon_dom.anon_node_hierarchy) {
            calc_block_width!(child_id, parent_content_size);
        }
    }
}

fn solve_heights<T: GetTextLayout>(
    root_size: LayoutSize,
    anon_dom: &AnonDom,
    anon_dom_depths: &NodeDepths,
    rect_contents: &mut BTreeMap<NodeId, RectContent<T>>,
    positioned_rects: &mut NodeDataContainer<PositionedRectangle>,
    resolved_text_layout_options: &BTreeMap<NodeId, (ResolvedTextLayoutOptions, InlineTextLayout)>,
) {
    use crate::style::PositionType;

    debug_assert!(anon_dom.anon_node_hierarchy.len() == positioned_rects.len());

    // First, distribute the known heights in a top-down fashion
    for (_depth, parent_node_id) in anon_dom_depths.iter() {

        let h = match &anon_dom.anon_node_data[*parent_node_id] {
            BlockNode(ref style) | InlineNode(ref style) => {
                match style.size.height {
                    Dimension::Undefined | Dimension::Auto => 0.0,
                    Dimension::Pixels(p) => p,
                    Dimension::Percent(pct) => {
                        let parent = anon_dom.anon_node_hierarchy[*parent_node_id].parent;
                        let parent_height = match parent {
                            None => root_size.height,
                            Some(s) => positioned_rects[s].size.height,
                        };
                        parent_height / 100.0 * pct
                    }
                }
            },
            AnonStyle => 0.0,
        };

        positioned_rects[*parent_node_id].size.height = h;
    }

    // Then, bubble the inline items up and increase the height if the
    // height isn't fixed.

    let mut content_heights = BTreeMap::new();
    let mut children_height_sum = 0.0;
    let mut current_depth_level = anon_dom_depths.last().map(|(s, _)| *s).unwrap_or(0);
    let mut last_parent = None;

    for (depth, parent_node_id) in anon_dom_depths.iter().rev() {

        last_parent = anon_dom.anon_node_hierarchy[*parent_node_id].parent;

        if current_depth_level != *depth {
            if let Some(last_parent) = last_parent {
                content_heights.insert(last_parent, children_height_sum);
            }
            last_parent = None;
            children_height_sum = 0.0;
            current_depth_level = *depth;
        }

        let parent_size = positioned_rects[*parent_node_id].size;

        for child_id in parent_node_id.children(&anon_dom.anon_node_hierarchy) {

            let child_anon_node = &anon_dom.anon_node_data[child_id];
            let child_position_type = child_anon_node.get_position_type();

            let self_width = positioned_rects[child_id].size.width;
            let children_content_height = child_id
                .children(&anon_dom.anon_node_hierarchy)
                .filter(|c| {
                    let position_type = anon_dom.anon_node_data[*c].get_position_type();
                    position_type != PositionType::Absolute && position_type != PositionType::Fixed
                })
                .map(|c| content_heights.get(&c).copied().unwrap_or(0.0))
                .sum();

            let block_height = match &anon_dom.anon_node_data[child_id] {
                BlockNode(ref style) => {
                    calculate_block_height(
                        style,
                        parent_size.width,
                        parent_size.height,
                        self_width,
                        children_content_height,
                        rect_contents.get_mut(&child_id),
                        resolved_text_layout_options.get(&child_id),
                    )
                },
                InlineNode(ref style) => {
                    calculate_block_height(
                        style,
                        parent_size.width,
                        parent_size.height,
                        self_width,
                        children_content_height,
                        rect_contents.get_mut(&child_id),
                        resolved_text_layout_options.get(&child_id),
                    )
                },
                AnonStyle => {
                    // set padding, margin, border to zero
                    BlockHeight { height: children_content_height, .. Default::default() }
                }
            };

            apply_block_height(block_height, &mut positioned_rects[child_id]);

            if child_position_type != PositionType::Absolute && child_position_type != PositionType::Fixed {
                children_height_sum += block_height.margin_box_height();
                // TODO: Why is this next line necessary?
                content_heights.insert(child_id, block_height.margin_box_height());
            }
        }
    }

    // If there is no defined height on the body node, set the height on the root node to be the
    // combined height of all child nodes.

    match &anon_dom.anon_node_data[NodeId::ZERO] {
        BlockNode(ref style) | InlineNode(ref style) => {
            let mut root_block_height = calculate_block_height(
                style,
                root_size.width,
                root_size.height,
                positioned_rects[NodeId::ZERO].size.width,
                children_height_sum,
                rect_contents.get_mut(&NodeId::ZERO),
                resolved_text_layout_options.get(&NodeId::ZERO),
            );
            root_block_height.height = root_block_height.height.max(root_size.height);
            apply_block_height(root_block_height, &mut positioned_rects[NodeId::ZERO]);
        },
        AnonStyle => {
            positioned_rects[NodeId::ZERO].size.height = children_height_sum.max(root_size.height);
        },
    }
}

fn position_items(
    root_size: LayoutSize,
    anon_dom: &AnonDom,
    anon_dom_depths: &NodeDepths,
    positioned_rects: &mut NodeDataContainer<PositionedRectangle>,
) {
    use crate::style::PositionType;

    // In order to do a block layout, we first have to know whether
    // can overflow the parent horizontally.

    let mut cur_x = 0.0;
    let mut cur_y = 0.0;

    // stack to track what the last position: absolute item was
    let mut position_relative_absolute_stack = Vec::new();
    let mut cur_depth = 0;

    for (depth, parent_node_id) in anon_dom_depths.iter() {

        cur_x = 0.0;

        // we are processing a new depth level
        if *depth != cur_depth {
            if position_relative_absolute_stack.last().map(|s: &(NodeId, usize)| s.1) == Some(cur_depth) {
                position_relative_absolute_stack.pop();
            }
            cur_y = 0.0;
            cur_depth = *depth;
        }

        let parent_width = match anon_dom.anon_node_hierarchy[*parent_node_id].parent {
            None => (root_size.width, StyleOverflow::Scroll),
            Some(s) => (positioned_rects[s].size.width, anon_dom.anon_node_data[s].get_overflow_x()),
        };

        let parent_content_size = positioned_rects[*parent_node_id].size.width;

        let parent_node = &anon_dom.anon_node_data[*parent_node_id];
        let parent_display_mode = parent_node.get_display();
        let parent_position_type = parent_node.get_position_type();

        if parent_position_type != PositionType::Static {
            position_relative_absolute_stack.push((*parent_node_id, *depth));
        }

        if parent_display_mode == Display::None {
            continue;
        }

        for child_id in parent_node_id.children(&anon_dom.anon_node_hierarchy) {

            let child_anon_node = &anon_dom.anon_node_data[child_id];
            let child_display_mode = child_anon_node.get_display();
            let child_position_type = child_anon_node.get_position_type();

            let child_rect = &mut positioned_rects[child_id];

            match child_display_mode {
                Display::None => continue,
                Display::Block | Display::InlineBlock | Display::Flex => {
                    let items_fit_in_line = ((cur_x + child_rect.get_margin_box_width().round()) as usize) < (parent_content_size.round() as usize);
                    if items_fit_in_line {
                        if child_display_mode == Display::Block {
                            cur_x = 0.0;
                        }

                        child_rect.position = PositionInfo::Static {
                            x_offset: cur_x + child_rect.get_left_leading(),
                            y_offset: cur_y + child_rect.get_top_leading(),
                        };

                        if child_display_mode == Display::Block {
                            cur_y += child_rect.get_margin_box_height();
                        } else {
                            // cur_y stays as it is
                            cur_x += child_rect.get_margin_box_width();
                        }
                    } else {
                        // items do not fit in line
                        child_rect.position = PositionInfo::Static {
                            x_offset: child_rect.get_left_leading(),
                            y_offset: cur_y + child_rect.get_top_leading(),
                        };
                        cur_x = 0.0; // line break
                        cur_y += child_rect.get_margin_box_height();
                    }

                    // if is_inline {
                    //      cur_x += inline_text_layout.last_line.origin.y + right_leading;
                    //
                    //      // if two following texts have different font sizes,
                    //      // align the two texts to their baseline
                    //
                    //      if last_inline_text.font_size > this_inline_text.font_size {
                    //           shift_down_first_line(this_inline_text, last_inline_text.baseline - this_inline_text.baseline);
                    //      } else if this_inline_text.font_size > last_inline_text.font_size {
                    //           shift_down_last_line(last_inline_text, this_inline_text.baseline - last_inline_text.baseline);
                    //      }
                    //      adjust_second_line_of_current_text(margin_bottom);
                    //      cur_y = inline_text_layout.last_line.origin.y + baseline_adjustment;
                    // }
                }
            }
        }
    }

    let mut position_relative_absolute_stack = Vec::new();
    let mut cur_depth = 0;

    // Right now all items are positioned relative to their parents, not to the page
    // Go down all nodes and add the positions together to get the final layout
    for (depth, parent_node_id) in anon_dom_depths.iter() {

        let parent_size = positioned_rects[*parent_node_id].size;
        let parent_anon_node = &anon_dom.anon_node_data[*parent_node_id];
        let parent_position_type = parent_anon_node.get_position_type();
        let parent_display_mode = parent_anon_node.get_display();

        if *depth != cur_depth {
            if position_relative_absolute_stack.last().map(|s: &(NodeId, usize)| s.1) == Some(cur_depth) {
                position_relative_absolute_stack.pop();
            }
            cur_depth = *depth;
        }

        if parent_position_type != PositionType::Static {
            position_relative_absolute_stack.push((*parent_node_id, *depth));
        }

        if parent_display_mode == Display::None {
            continue;
        }

        for child_id in parent_node_id.children(&anon_dom.anon_node_hierarchy) {

            let child_anon_node = &anon_dom.anon_node_data[child_id];
            let child_position_type = child_anon_node.get_position_type();

            let previous_sibling_origin = get_previous_sibling_origin(&anon_dom.anon_node_hierarchy, &positioned_rects, child_id);

            match child_position_type {
                PositionType::Static => {
                    // item already positioned
                },
                PositionType::Fixed => {
                    let child_rect = &mut positioned_rects[child_id];
                    let new_bounds = figure_out_position(root_size, previous_sibling_origin, child_anon_node.get_style());
                    child_rect.size = new_bounds.size;
                    child_rect.position = PositionInfo::Fixed {
                        x_offset: new_bounds.origin.x,
                        y_offset: new_bounds.origin.y,
                    };
                },
                PositionType::Relative => {

                    let child_rect = &mut positioned_rects[child_id];
                    let child_style = child_anon_node.get_style();

                    let top_offset = child_style.position.top.resolve(Defined(parent_size.height)).unwrap_or_zero();
                    let top_is_defined = child_style.position.top.is_defined();

                    let bottom_offset = child_style.position.bottom.resolve(Defined(parent_size.height)).unwrap_or_zero();
                    let bottom_is_defined = child_style.position.bottom.is_defined();

                    let left_offset = child_style.position.left.resolve(Defined(parent_size.width)).unwrap_or_zero();
                    let left_is_defined = child_style.position.left.is_defined();

                    let right_offset = child_style.position.right.resolve(Defined(parent_size.width)).unwrap_or_zero();
                    let right_is_defined = child_style.position.right.is_defined();

                    let x_offset = if left_is_defined {
                        left_offset
                    } else if right_is_defined {
                        parent_size.width - child_rect.size.width - right_offset
                    } else {
                        previous_sibling_origin.x
                    };

                    let y_offset = if top_is_defined {
                        top_offset
                    } else if bottom_is_defined {
                        parent_size.height - child_rect.size.height - bottom_offset
                    } else {
                        previous_sibling_origin.y
                    };

                    child_rect.position = PositionInfo::Relative { x_offset, y_offset };
                },
                PositionType::Absolute => {
                    let last_positioned_node = position_relative_absolute_stack.last().copied().unwrap_or((NodeId::ZERO, 0));
                    let last_positioned_size = positioned_rects[last_positioned_node.0].size;
                    let child_rect = &mut positioned_rects[child_id];
                    let computed_bounds = figure_out_position(last_positioned_size, previous_sibling_origin, child_anon_node.get_style());
                    child_rect.size.width = computed_bounds.size.width;
                    child_rect.size.height = computed_bounds.size.height;
                    child_rect.position = PositionInfo::Absolute {
                        x_offset: computed_bounds.origin.x,
                        y_offset: computed_bounds.origin.y,
                    };
                },
            }
        }
    }
}

fn get_previous_sibling_origin(
    node_hierarchy: &NodeHierarchy,
    positioned_rects: &NodeDataContainer<PositionedRectangle>,
    child_id: NodeId,
) -> LayoutPoint {

    let mut cur_point = LayoutPoint::zero();
    let mut cur_previous_sibling = node_hierarchy[child_id].previous_sibling;

    while let Some(ps) = cur_previous_sibling {
        match positioned_rects[ps].position {
            PositionInfo::Static { x_offset, y_offset } => {
                cur_point = LayoutPoint::new(x_offset, y_offset);
                break;
            },
            PositionInfo::Relative { x_offset, y_offset } => {
                cur_point = LayoutPoint::new(x_offset, y_offset);
                break;
            },
            PositionInfo::Absolute { .. } | PositionInfo::Fixed { .. } => {
                cur_previous_sibling = node_hierarchy[ps].previous_sibling;
            },
        }
    }

    cur_point
}

// Figure out the origin of a position:absolute rect, given
fn figure_out_position(
    parent_size: LayoutSize,
    previous_sibling_origin: LayoutPoint,
    child_style: &Style,
) -> LayoutRect {

    let top_offset = child_style.position.top.resolve(Defined(parent_size.height)).unwrap_or_zero();
    let top_is_defined = child_style.position.top.is_defined();

    let bottom_offset = child_style.position.bottom.resolve(Defined(parent_size.height)).unwrap_or_zero();
    let bottom_is_defined = child_style.position.bottom.is_defined();

    let left_offset = child_style.position.left.resolve(Defined(parent_size.width)).unwrap_or_zero();
    let left_is_defined = child_style.position.left.is_defined();

    let right_offset = child_style.position.right.resolve(Defined(parent_size.width)).unwrap_or_zero();
    let right_is_defined = child_style.position.right.is_defined();

    // If the width of the item is undefined, the width is defined by the left: / right: attributes
    let width_is_defined = child_style.size.width.is_defined();
    let width =
        if width_is_defined {
            child_style.size.width.resolve(Defined(parent_size.width))
        } else if right_is_defined && left_is_defined {
            Defined(parent_size.width - left_offset - right_offset)
        } else {
            Defined(0.0)
        }
    .maybe_min(child_style.min_size.width.resolve(Defined(parent_size.width)))
    .maybe_max(child_style.max_size.width.resolve(Defined(parent_size.width)))
    .unwrap_or_zero();

    // Same process for height
    let height_is_defined = child_style.size.height.is_defined();
    let height = if height_is_defined {
        child_style.size.height.resolve(Defined(parent_size.height))
    } else if top_is_defined && bottom_is_defined {
        Defined(parent_size.height - top_offset - bottom_offset)
    } else {
        Defined(0.0)
    }
    .maybe_min(child_style.min_size.height.resolve(Defined(parent_size.height)))
    .maybe_max(child_style.max_size.height.resolve(Defined(parent_size.height)))
    .unwrap_or_zero();

    // Now figure out the offset of the rectangle (TODO: incorporate padding / margin)!:
    let x_offset = if left_is_defined {
        left_offset
    } else if right_is_defined {
        parent_size.width - width - right_offset
    } else {
        previous_sibling_origin.x
    };

    let y_offset = if top_is_defined {
        top_offset
    } else if bottom_is_defined {
        parent_size.height - height - bottom_offset
    } else {
        previous_sibling_origin.y
    };

    LayoutRect {
        origin: LayoutPoint::new(x_offset, y_offset),
        size: LayoutSize::new(width, height),
    }
}

/*
    // Get the collapsed (left, right) margins of this node, i.e. the larger of the two margins
    //
    // NOTE: Only use when using horizontal layouts, not block layouts!
    fn get_collapsed_horz_margin(
        node_id: NodeId,
        anon_node_hierarchy: &NodeHierarchy,
        positioned_rects: &NodeDataContainer<PositionedRectangle>,
    ) -> (f32, f32) {

        let previous_sibling = &anon_node_hierarchy[node_id].previous_sibling;
        let next_sibling = &anon_node_hierarchy[node_id].next_sibling;
        let positioned_rect = &positioned_rects[node_id];
        let (margin_right, margin_left) = (positioned_rect.margin.right, positioned_rect.margin.left);

        let previous_sibling_margin_right = previous_sibling.map(|ps| positioned_rects[ps].margin.right).unwrap_or(0.0);
        let next_sibling_margin_left = next_sibling.map(|ps| positioned_rects[ps].margin.left).unwrap_or(0.0);

        (previous_sibling_margin_right.max(margin_left), next_sibling_margin_left.max(margin_right))
    }

    // Get the collapsed (top, bottom) margins of this node, i.e. the larger of the two margins
    fn get_collapsed_vert_margin(
        node_id: NodeId,
        anon_node_hierarchy: &NodeHierarchy,
        positioned_rects: &NodeDataContainer<PositionedRectangle>,
    ) -> (f32, f32) {

        let previous_sibling = &anon_node_hierarchy[node_id].previous_sibling;
        let next_sibling = &anon_node_hierarchy[node_id].next_sibling;
        let positioned_rect = &positioned_rects[node_id];
        let (margin_top, margin_bottom) = (positioned_rect.margin.top, positioned_rect.margin.bottom);

        let previous_sibling_margin_bottom = previous_sibling.map(|ps| positioned_rects[ps].margin.bottom).unwrap_or(0.0);
        let next_sibling_margin_top = next_sibling.map(|ps| positioned_rects[ps].margin.top).unwrap_or(0.0);

        (previous_sibling_margin_bottom.max(margin_top), next_sibling_margin_top.max(margin_bottom))
    }
*/

// ------

#[derive(Copy, Clone, Default)]
struct BlockWidth {
    width: f32,
    border_width: (f32, f32),
    margin: (f32, f32),
    padding: (f32, f32),
}

impl fmt::Debug for BlockWidth {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BlockWidth {{\r\n")?;
        write!(f, "    width: {}\r\n", self.width)?;
        write!(f, "    border_width: {:?}\r\n", self.border_width)?;
        write!(f, "    margin: {:?}\r\n", self.margin)?;
        write!(f, "    padding: {:?}\r\n", self.padding)?;
        write!(f, "}}")?;
        Ok(())
    }
}

impl BlockWidth {
    #[inline]
    pub fn content_box_width(&self) -> f32 { self.width }
    #[inline]
    pub fn padding_box_width(&self) -> f32 { self.content_box_width() + self.padding.0 + self.padding.1 }
    #[inline]
    pub fn border_box_width(&self) -> f32 { self.padding_box_width() + self.border_width.0 + self.border_width.1 }
    #[inline]
    pub fn margin_box_width(&self) -> f32 { self.border_box_width() + self.margin.0 + self.margin.1 }
}

fn check_width_minmax(width: Number, style: &Style, pw: f32) -> Number {
    let pwn = Defined(pw);
    width
    .maybe_min(style.max_size.width.resolve(pwn))
    .maybe_max(style.min_size.width.resolve(pwn))
    .maybe_min(if style.min_size.width.resolve(pwn).unwrap_or_zero() > style.max_size.width.resolve(pwn).to_option().unwrap_or(pw) {
        style.min_size.width.resolve(pwn)
    } else { Undefined })
}

fn check_height_minmax(height: Number, style: &Style, ph: f32) -> Number {
    let phn = Defined(ph);
    height
    .maybe_min(style.max_size.height.resolve(phn))
    .maybe_max(style.min_size.height.resolve(phn))
    .maybe_min(if style.min_size.height.resolve(phn).unwrap_or_zero() > style.max_size.height.resolve(phn).to_option().unwrap_or(ph) {
        style.min_size.height.resolve(phn)
    } else { Undefined })
}

// see: https://limpet.net/mbrubeck/2014/09/17/toy-layout-engine-6-block.html
fn calculate_block_width(
    style: &Style,
    parent_content_width: f32,
) -> BlockWidth {

    if style.display == Display::None {
        return BlockWidth::default();
    }

    let pw = Defined(parent_content_width);

    // The default for block width is 100% of the parent width
    let width = style.size.width.resolve(pw).or_else(parent_content_width);
    let width_is_defined = style.size.width.is_defined();

    let mut margin_left = style.margin.left.resolve(pw).or_else(0.0);
    let mut margin_right = style.margin.right.resolve(pw).or_else(0.0);

    let padding_left = style.padding.left.resolve(pw).or_else(0.0);
    let padding_right = style.padding.right.resolve(pw).or_else(0.0);

    let border_width_left = style.border.left.resolve(pw).or_else(0.0);
    let border_width_right = style.border.right.resolve(pw).or_else(0.0);

    // Adjust for min / max width properties
    let mut width = check_width_minmax(Defined(match style.box_sizing {
        // The width and height properties (and min/max properties) includes only the content.
        BoxSizing::ContentBox => width,
        // The width and height properties (and min/max properties) includes content, padding and border
        BoxSizing::BorderBox => {
            if width_is_defined {
                width + padding_left + padding_right + border_width_left + border_width_right
            } else {
                width
            }
        },
    }), style, parent_content_width)
    .or_else(width)
    - margin_left - margin_right - padding_left - padding_right - border_width_left - border_width_right;

    let total_width = width
        + margin_left
        + margin_right
        + padding_left
        + padding_right
        + border_width_left
        + border_width_right;

    if style.size.width != Dimension::Auto && total_width > parent_content_width {
        if style.margin.left == Dimension::Auto {
            margin_left = 0.0;
        }
        if style.margin.right == Dimension::Auto {
            margin_right = 0.0;
        }
    }

    let underflow = parent_content_width - total_width;

    match (
        style.size.width == Dimension::Auto,
        style.margin.left == Dimension::Auto,
        style.margin.right == Dimension::Auto
    ) {
        // If the values are overconstrained, calculate margin_right.
        (false, false, false) => {
            margin_right += underflow;
        }

        // If exactly one size is auto, its used value follows from the equality.
        (false, false, true) => { margin_right = underflow; }
        (false, true, false) => { margin_left  = underflow; }

        // If width is set to auto, any other auto values become 0.
        (true, _, _) => {
            if style.margin.left == Dimension::Auto { margin_left = 0.0; }
            if style.margin.right == Dimension::Auto { margin_right = 0.0; }

            if underflow >= 0.0 {
                // Expand width to fill the underflow.
                width = underflow;
            } else {
                // Width can't be negative. Adjust the right margin instead.
                width = 0.0;
                margin_right += underflow;
            }
        }

        // If margin-left and margin-right are both auto, their used values are equal.
        (false, true, true) => {
            margin_left = underflow / 2.0;
            margin_right = underflow / 2.0;
        }
    }

    BlockWidth {
        width,
        border_width: (border_width_left, border_width_right),
        margin: (margin_left, margin_right),
        padding: (padding_left, padding_right),
    }
}

#[derive(Copy, Clone, Default)]
struct InlineWidth {
    width: f32,
    border_width: (f32, f32),
    margin: (f32, f32),
    padding: (f32, f32),
    trailing: Option<f32>,
}

impl fmt::Debug for InlineWidth {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "InlineWidth {{\r\n")?;
        write!(f, "    width: {}\r\n", self.width)?;
        write!(f, "    border_width: {:?}\r\n", self.border_width)?;
        write!(f, "    margin: {:?}\r\n", self.margin)?;
        write!(f, "    padding: {:?}\r\n", self.padding)?;
        write!(f, "    trailing: {:?}\r\n", self.trailing)?;
        write!(f, "}}")?;
        Ok(())
    }
}

impl InlineWidth {
    #[inline]
    pub fn content_box_width(&self) -> f32 { self.width }
    #[inline]
    pub fn padding_box_width(&self) -> f32 { self.content_box_width() + self.padding.0 + self.padding.1 }
    #[inline]
    pub fn border_box_width(&self) -> f32 { self.padding_box_width() + self.border_width.0 + self.border_width.1 }
    #[inline]
    pub fn margin_box_width(&self) -> f32 { self.border_box_width() + self.margin.0 + self.margin.1 }
}

fn calculate_inline_width<T: GetTextLayout>(
    node_id: NodeId,
    last_leading: Option<f32>,
    rect_content: Option<&mut RectContent<T>>,
    style: &Style,
    parent_content_width: f32,
    parent_overflow_x: StyleOverflow,
    resolved_text_layout_options: &mut BTreeMap<NodeId, (ResolvedTextLayoutOptions, InlineTextLayout)>,
) -> InlineWidth {

    if style.display == Display::None {
        return InlineWidth::default();
    }

    let pw = Defined(parent_content_width);

    let mut leading = None;

    // The inline block has a different width calculation
    let width = match (style.size.width.resolve(pw), rect_content) {
        (Defined(f), _) => f,
        (Undefined, None) => 0.0,
        (Undefined, Some(Image(image_width, image_height))) => {
                *image_width as f32 / *image_height as f32
                * check_width_minmax(style.size.width.resolve(pw), style, parent_content_width).or_else(parent_content_width)
        },
        (Undefined, Some(Text(t))) => {
            use azul_core::ui_solver::{
                DEFAULT_FONT_SIZE_PX, DEFAULT_LETTER_SPACING,
                DEFAULT_WORD_SPACING,
            };

            let text_layout_options = ResolvedTextLayoutOptions {
                max_horizontal_width: if parent_overflow_x.allows_horizontal_overflow() { None } else { Some(parent_content_width) },
                leading: last_leading,
                holes: Vec::new(),
                font_size_px: style.font_size_px.to_pixels(DEFAULT_FONT_SIZE_PX as f32),
                letter_spacing: style.letter_spacing.map(|ls| ls.to_pixels(DEFAULT_LETTER_SPACING)),
                word_spacing: style.word_spacing.map(|ls| ls.to_pixels(DEFAULT_WORD_SPACING)),
                line_height: style.line_height,
                tab_width: style.tab_width,
            };

            let layouted_inline_text = t.get_text_layout(&text_layout_options);

            leading = Some(layouted_inline_text.get_trailing());

            let inline_text_bounds = layouted_inline_text.get_bounds();

            resolved_text_layout_options.insert(node_id, (text_layout_options, layouted_inline_text));

            inline_text_bounds.size.width
        },
    };

    let margin_left = style.margin.left.resolve(pw).or_else(0.0);
    let margin_right = style.margin.right.resolve(pw).or_else(0.0);

    let padding_left = style.padding.left.resolve(pw).or_else(0.0);
    let padding_right = style.padding.right.resolve(pw).or_else(0.0);

    let border_width_left = style.border.left.resolve(pw).or_else(0.0);
    let border_width_right = style.border.right.resolve(pw).or_else(0.0);

    // Adjust for min / max width properties
    let width = check_width_minmax(Defined(match style.box_sizing {
        // The width and height properties (and min/max properties) includes only the content.
        BoxSizing::BorderBox => width + padding_left + padding_right + border_width_left + border_width_right,
        // The width and height properties (and min/max properties) includes content, padding and border
        BoxSizing::ContentBox => width,
    }), style, parent_content_width)
    .or_else(width)
    - padding_left - padding_right - border_width_left - border_width_right;

    InlineWidth {
        width,
        border_width: (border_width_left, border_width_right),
        margin: (margin_left, margin_right),
        padding: (padding_left, padding_right),
        trailing: if style.display == Display::InlineBlock { leading } else { None }
    }
}

fn apply_block_width(block: BlockWidth, node: &mut PositionedRectangle) {
    node.size.width = block.width;
    node.padding.left = block.padding.0;
    node.padding.right = block.padding.1;
    node.margin.left = block.margin.0;
    node.margin.right = block.margin.1;
    node.border_widths.left = block.border_width.0;
    node.border_widths.right = block.border_width.1;
}

// ----

#[derive(Debug, Copy, Clone, Default)]
struct BlockHeight {
    height: f32,
    content_height: Option<f32>,
    border_height: (f32, f32),
    margin: (f32, f32),
    padding: (f32, f32),
}

impl BlockHeight {
    #[inline]
    pub fn content_box_height(&self) -> f32 { self.height }
    #[inline]
    pub fn padding_box_height(&self) -> f32 { self.content_box_height() + self.padding.0 + self.padding.1 }
    #[inline]
    pub fn border_box_height(&self) -> f32 { self.padding_box_height() + self.border_height.0 + self.border_height.1 }
    #[inline]
    pub fn margin_box_height(&self) -> f32 { self.border_box_height() + self.margin.0 + self.margin.1 }
}

fn calculate_block_height<T: GetTextLayout>(
    style: &Style,
    parent_width: f32,
    parent_height: f32,
    block_width: f32,
    children_content_height: f32,
    rect_content: Option<&mut RectContent<T>>,
    resolved_text_layout_options: Option<&(ResolvedTextLayoutOptions, InlineTextLayout)>,
) -> BlockHeight {

    if style.display == Display::None {
        return BlockHeight::default();
    }

    let ph = Defined(parent_height);

    let mut content_height = None;
    let mut height_is_defined = false;

    let height = match style.size.height {
        Dimension::Undefined | Dimension::Auto => {
            let self_content_height = match rect_content {
                None => None,
                Some(Image(image_width, image_height)) => {
                    Some(*image_width as f32 / *image_height as f32 * block_width)
                },
                Some(Text(t)) => {
                    match resolved_text_layout_options {
                        None => None,
                        Some((tlo, layouted_inline_text)) => {
                            let inline_text_bounds = layouted_inline_text.get_bounds();
                            Some(inline_text_bounds.size.height)
                        }
                    }
                },
            };

            content_height = self_content_height;
            children_content_height.max(self_content_height.unwrap_or(0.0))
        },
        Dimension::Pixels(p) => { height_is_defined = true; p },
        Dimension::Percent(pct) => {
            height_is_defined = true;
            parent_height / 100.0 * pct
        }
    };

    let pw = Defined(parent_width);

    // CSS spec:
    //
    // The percentage is calculated with respect to the *width* of the generated box’s
    // containing block. Note that this is true for margin-top and margin-bottom as well.

    let margin_top = style.margin.top.resolve(pw).or_else(0.0);
    let margin_bottom = style.margin.bottom.resolve(pw).or_else(0.0);

    let padding_top = style.padding.top.resolve(pw).or_else(0.0);
    let padding_bottom = style.padding.bottom.resolve(pw).or_else(0.0);

    let border_height_top = style.border.top.resolve(pw).or_else(0.0);
    let border_height_bottom = style.border.bottom.resolve(pw).or_else(0.0);

    // Adjust for min / max height properties
    let height = check_height_minmax(Defined(match style.box_sizing {
        // The width and height properties (and min/max properties) includes only the content.
        BoxSizing::ContentBox => height + padding_top + padding_bottom + border_height_top + border_height_bottom,
        // The width and height properties (and min/max properties) includes content, padding and border
        BoxSizing::BorderBox => if height_is_defined {
            height
        } else {
            height + padding_top + padding_bottom + border_height_top + border_height_bottom
        },
    }), style, parent_height)
    .or_else(height)
    - padding_top - padding_bottom - border_height_top - border_height_bottom;

    BlockHeight {
        height,
        content_height,
        border_height: (border_height_top, border_height_bottom),
        margin: (margin_top, margin_bottom),
        padding: (padding_top, padding_bottom),
    }
}

fn apply_block_height(block: BlockHeight, node: &mut PositionedRectangle) {
    node.size.height = block.height;
    node.padding.top = block.padding.0;
    node.padding.bottom = block.padding.1;
    node.margin.top = block.margin.0;
    node.margin.bottom = block.margin.1;
    node.border_widths.top = block.border_height.0;
    node.border_widths.bottom = block.border_height.1;
}
