use std::{f32, collections::BTreeMap};
use azul_css::{
    LayoutPosition, LayoutPoint, LayoutSize, LayoutRect,
    LayoutMarginTop, LayoutMarginRight, LayoutMarginBottom, LayoutMarginLeft,
    LayoutPaddingTop, LayoutPaddingRight, LayoutPaddingBottom, LayoutPaddingLeft,
    RectLayout, StyleTextAlignmentHorz, StyleTextAlignmentVert, CssPropertyValue,
};
use azul_core::{
    traits::GetTextLayout,
    id_tree::{NodeId, NodeDataContainer, NodeHierarchy},
    display_list::DisplayRectangle,
    dom::{NodeData, NodeType},
    ui_solver::{DEFAULT_FONT_SIZE_PX, ResolvedOffsets, LayoutResult, PositionedRectangle, OverflowInfo},
    app_resources::{AppResources, FontInstanceKey},
    callbacks::PipelineId,
};
use azul_text_layout::{InlineText, text_layout::{Words, ScaledWords, WordPositions}};

const DEFAULT_FLEX_GROW_FACTOR: f32 = 1.0;

#[derive(Debug, Copy, Clone, PartialEq)]
enum WhConstraint {
    /// between min, max
    Between(f32, f32),
    /// Value needs to be exactly X
    EqualTo(f32),
    /// Value can be anything
    Unconstrained,
}

impl Default for WhConstraint {
    fn default() -> Self { WhConstraint::Unconstrained }
}

impl WhConstraint {

    /// Returns the minimum value or 0 on `Unconstrained`
    /// (warning: this might not be what you want)
    pub fn min_needed_space(&self) -> Option<f32> {
        use self::WhConstraint::*;
        match self {
            Between(min, _) => Some(*min),
            EqualTo(exact) => Some(*exact),
            Unconstrained => None,
        }
    }

    /// Returns the maximum space until the constraint is violated - returns
    /// `None` if the constraint is unbounded
    pub fn max_available_space(&self) -> Option<f32> {
        use self::WhConstraint::*;
        match self {
            Between(_, max) => { Some(*max) },
            EqualTo(exact) => Some(*exact),
            Unconstrained => None,
        }
    }

    /// Returns if this `WhConstraint` is an `EqualTo` constraint
    pub fn is_fixed_constraint(&self) -> bool {
        use self::WhConstraint::*;
        match self {
            EqualTo(_) => true,
            _ => false,
        }
    }
}

macro_rules! determine_preferred {
    ($fn_name:ident, $width:ident, $min_width:ident, $max_width:ident) => (

    /// - `preferred_inner_width` denotes the preferred width of the width or height got from the
    /// from the rectangles content.
    ///
    /// For example, if you have an image, the `preferred_inner_width` is the images width,
    /// if the node type is an text, the `preferred_inner_width` is the text height.
    fn $fn_name(layout: &RectLayout, preferred_inner_width: Option<f32>, parent_width: f32) -> WhConstraint {

        let mut width = layout.$width.and_then(|w| w.get_property().map(|x| x.inner.to_pixels(parent_width)));
        let min_width = layout.$min_width.and_then(|w| w.get_property().map(|x| x.inner.to_pixels(parent_width)));
        let max_width = layout.$max_width.and_then(|w| w.get_property().map(|x| x.inner.to_pixels(parent_width)));

        // TODO: correct for width / height less than 0 - "negative" width is impossible!

        let (absolute_min, absolute_max) = {
            if let (Some(min), Some(max)) = (min_width, max_width) {
                if min_width < max_width {
                    (Some(min), Some(max))
                } else {
                    // min-width > max_width: max_width wins
                    (Some(max), Some(max))
                }
            } else {
                (min_width, max_width)
            }
        };

        // We only need to correct the width if the preferred width is in the range
        // between min & max and the width isn't already specified as a style
        if let Some(preferred_width) = preferred_inner_width {
            if width.is_none() &&
               preferred_width > absolute_min.unwrap_or(0.0) &&
               preferred_width < absolute_max.unwrap_or(f32::MAX)
            {
                width = Some(preferred_width);
            }
        };

        if let Some(width) = width {
            if let Some(max_width) = absolute_max {
                if let Some(min_width) = absolute_min {
                    if min_width < width && width < max_width {
                        // normal: min_width < width < max_width
                        WhConstraint::EqualTo(width)
                    } else if width > max_width {
                        WhConstraint::EqualTo(max_width)
                    } else if width < min_width {
                        WhConstraint::EqualTo(min_width)
                    } else {
                        WhConstraint::Unconstrained /* unreachable */
                    }
                } else {
                    // width & max_width
                    WhConstraint::EqualTo(width.min(max_width))
                }
            } else if let Some(min_width) = absolute_min {
                // no max width, only width & min_width
                WhConstraint::EqualTo(width.max(min_width))
            } else {
                // no min-width or max-width
                WhConstraint::EqualTo(width)
            }
        } else {
            // no width, only min_width and max_width
            if let Some(max_width) = absolute_max {
                if let Some(min_width) = absolute_min {
                    WhConstraint::Between(min_width, max_width)
                } else {
                    // TODO: check sign positive on max_width!
                    WhConstraint::Between(0.0, max_width)
                }
            } else {
                if let Some(min_width) = absolute_min {
                    WhConstraint::Between(min_width, f32::MAX)
                } else {
                    // no width, min_width or max_width
                    WhConstraint::Unconstrained
                }
            }
        }
    })
}

// Returns the preferred width, given [width, min_width, max_width] inside a RectLayout
// or `None` if the height can't be determined from the node alone.
//
// fn determine_preferred_width(layout: &RectLayout) -> Option<f32>
determine_preferred!(determine_preferred_width, width, min_width, max_width);

// Returns the preferred height, given [height, min_height, max_height] inside a RectLayout
// or `None` if the height can't be determined from the node alone.
//
// fn determine_preferred_height(layout: &RectLayout) -> Option<f32>
determine_preferred!(determine_preferred_height, height, min_height, max_height);

#[derive(Debug, Default, Copy, Clone, PartialEq)]
struct WidthCalculatedRect {
    pub preferred_width: WhConstraint,
    pub margin_top: CssPropertyValue<LayoutMarginTop>,
    pub margin_right: CssPropertyValue<LayoutMarginRight>,
    pub margin_left: CssPropertyValue<LayoutMarginLeft>,
    pub margin_bottom: CssPropertyValue<LayoutMarginBottom>,
    pub padding_top: CssPropertyValue<LayoutPaddingTop>,
    pub padding_left: CssPropertyValue<LayoutPaddingLeft>,
    pub padding_right: CssPropertyValue<LayoutPaddingRight>,
    pub padding_bottom: CssPropertyValue<LayoutPaddingBottom>,
    pub flex_grow_px: f32,
    pub min_inner_size_px: f32,
}

impl WidthCalculatedRect {
    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_horizontal(&self, parent_width: f32) -> f32 {
        self.preferred_width.min_needed_space().unwrap_or(0.0) +
        self.margin_left.get_property().map(|px| px.inner.to_pixels(parent_width)).unwrap_or(0.0) +
        self.margin_right.get_property().map(|px| px.inner.to_pixels(parent_width)).unwrap_or(0.0) +
        self.padding_left.get_property().map(|px| px.inner.to_pixels(parent_width)).unwrap_or(0.0) +
        self.padding_right.get_property().map(|px| px.inner.to_pixels(parent_width)).unwrap_or(0.0)
    }

    /// Get the sum of the horizontal padding amount (`padding.left + padding.right`)
    pub fn get_horizontal_padding(&self, parent_width: f32) -> f32 {
        self.padding_left.get_property().map(|px| px.inner.to_pixels(parent_width)).unwrap_or(0.0) +
        self.padding_right.get_property().map(|px| px.inner.to_pixels(parent_width)).unwrap_or(0.0)
    }

    /// Called after solver has run: Solved width of rectangle
    pub fn solved_result(&self) -> WidthSolvedResult {
        WidthSolvedResult {
            min_width: self.min_inner_size_px,
            space_added: self.flex_grow_px,
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
struct HeightCalculatedRect {
    pub preferred_height: WhConstraint,
    pub margin_top: CssPropertyValue<LayoutMarginTop>,
    pub margin_right: CssPropertyValue<LayoutMarginRight>,
    pub margin_left: CssPropertyValue<LayoutMarginLeft>,
    pub margin_bottom: CssPropertyValue<LayoutMarginBottom>,
    pub padding_top: CssPropertyValue<LayoutPaddingTop>,
    pub padding_left: CssPropertyValue<LayoutPaddingLeft>,
    pub padding_right: CssPropertyValue<LayoutPaddingRight>,
    pub padding_bottom: CssPropertyValue<LayoutPaddingBottom>,
    pub flex_grow_px: f32,
    pub min_inner_size_px: f32,
}

impl HeightCalculatedRect {
    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_vertical(&self, parent_height: f32) -> f32 {
        self.preferred_height.min_needed_space().unwrap_or(0.0) +
        self.margin_top.get_property().map(|px| px.inner.to_pixels(parent_height)).unwrap_or(0.0) +
        self.margin_bottom.get_property().map(|px| px.inner.to_pixels(parent_height)).unwrap_or(0.0) +
        self.padding_top.get_property().map(|px| px.inner.to_pixels(parent_height)).unwrap_or(0.0) +
        self.padding_bottom.get_property().map(|px| px.inner.to_pixels(parent_height)).unwrap_or(0.0)
    }

    /// Get the sum of the horizontal padding amount (`padding_top + padding_bottom`)
    pub fn get_vertical_padding(&self, parent_height: f32) -> f32 {
        self.padding_top.get_property().map(|px| px.inner.to_pixels(parent_height)).unwrap_or(0.0) +
        self.padding_bottom.get_property().map(|px| px.inner.to_pixels(parent_height)).unwrap_or(0.0)
    }

    /// Called after solver has run: Solved width of rectangle
    pub fn solved_result(&self) -> HeightSolvedResult {
        HeightSolvedResult {
            min_height: self.min_inner_size_px,
            space_added: self.flex_grow_px,
        }
    }
}

/// ```rust
/// typed_arena!(
///     WidthCalculatedRect,
///     preferred_width,
///     determine_preferred_width,
///     get_horizontal_padding,
///     get_flex_basis_horizontal,
///     width_calculated_rect_arena_from_rect_layout_arena,
///     bubble_preferred_widths_to_parents,
///     width_calculated_rect_arena_apply_flex_grow,
///     width_calculated_rect_arena_sum_children_flex_basis,
///     Horizontal,
/// )
/// ```
macro_rules! typed_arena {(
    $struct_name:ident,
    $preferred_field:ident,
    $determine_preferred_fn:ident,
    $get_padding_fn:ident,
    $get_flex_basis:ident,
    $from_rect_layout_arena_fn_name:ident,
    $bubble_fn_name:ident,
    $apply_flex_grow_fn_name:ident,
    $sum_children_flex_basis_fn_name:ident,
    $main_axis:ident
) => (


    /// Fill out the preferred width of all nodes.
    ///
    /// We could operate on the Arena<DisplayRectangle> directly, but that makes testing very
    /// hard since we are only interested in testing or touching the layout. So this makes the
    /// calculation maybe a few microseconds slower, but gives better testing capabilities
    ///
    /// NOTE: Later on, this could maybe be a NodeDataContainer<&'a RectLayout>.
    #[must_use]
    fn $from_rect_layout_arena_fn_name(
        node_data: &NodeDataContainer<RectLayout>,
        widths: &NodeDataContainer<Option<f32>>,
        node_hierarchy: &NodeHierarchy
    ) -> NodeDataContainer<$struct_name> {

        // then calculate the widths again, but this time using the parent nodes
        let mut new_nodes = NodeDataContainer {
            internal: vec![$struct_name::default();node_data.len()]
        };

        for (_, parent_id) in node_hierarchy.get_parents_sorted_by_depth() {

            let nd = &node_data[parent_id];
            let width = match widths.get(parent_id) { Some(s) => *s, None => continue, };
            let parent_width = node_hierarchy
            .get(parent_id)
            .and_then(|t| new_nodes.get(t.parent?))
            .map(|parent| parent.$preferred_field)
            .unwrap_or_default()
            .min_needed_space()
            .unwrap_or(0.0);
            let parent_width = $determine_preferred_fn(&nd, width, parent_width);

            new_nodes[parent_id] = $struct_name {
                // TODO: get the initial width of the rect content
                $preferred_field: parent_width,

                margin_top: nd.margin_top.unwrap_or_default(),
                margin_left: nd.margin_left.unwrap_or_default(),
                margin_right: nd.margin_right.unwrap_or_default(),
                margin_bottom: nd.margin_bottom.unwrap_or_default(),

                padding_top: nd.padding_top.unwrap_or_default(),
                padding_left: nd.padding_left.unwrap_or_default(),
                padding_right: nd.padding_right.unwrap_or_default(),
                padding_bottom: nd.padding_bottom.unwrap_or_default(),

                flex_grow_px: 0.0,
                min_inner_size_px: 0.0,
            };

            for child_id in parent_id.children(node_hierarchy) {
                let nd = &node_data[child_id];
                let width = match widths.get(child_id) { Some(s) => *s, None => continue, };
                new_nodes[child_id] = $struct_name {
                    // TODO: get the initial width of the rect content
                    $preferred_field: $determine_preferred_fn(&nd, width, parent_width.min_needed_space().unwrap_or(0.0)),

                    margin_top: nd.margin_top.unwrap_or_default(),
                    margin_left: nd.margin_left.unwrap_or_default(),
                    margin_right: nd.margin_right.unwrap_or_default(),
                    margin_bottom: nd.margin_bottom.unwrap_or_default(),

                    padding_top: nd.padding_top.unwrap_or_default(),
                    padding_left: nd.padding_left.unwrap_or_default(),
                    padding_right: nd.padding_right.unwrap_or_default(),
                    padding_bottom: nd.padding_bottom.unwrap_or_default(),

                    flex_grow_px: 0.0,
                    min_inner_size_px: 0.0,
                }
            }
        }

        new_nodes
    }

    /// Bubble the inner sizes to their parents -  on any parent nodes, fill out
    /// the width so that the `preferred_width` can contain the child nodes (if
    /// that doesn't violate the constraints of the parent)
    fn $bubble_fn_name(
        node_data: &mut NodeDataContainer<$struct_name>,
        node_hierarchy: &NodeHierarchy,
        arena_data: &NodeDataContainer<RectLayout>,
        non_leaf_nodes: &[(usize, NodeId)])
    {
        // Reverse, since we want to go from the inside out (depth 5 needs to be filled out first)
        //
        // Set the preferred_width of the parent nodes
        for (_node_depth, non_leaf_id) in non_leaf_nodes.iter().rev() {

            use self::WhConstraint::*;

            let parent_width = node_hierarchy[*non_leaf_id].parent.and_then(|parent_id| node_data[parent_id].$preferred_field.min_needed_space()).unwrap_or(0.0);

            // Sum of the direct children's flex-basis = the parents preferred width
            let children_flex_basis = $sum_children_flex_basis_fn_name(node_data, *non_leaf_id, node_hierarchy, arena_data);

            // Calculate the new flex-basis width
            let parent_width_metrics = node_data[*non_leaf_id];

            // For calculating the inner width, subtract the parents padding
            let parent_padding = node_data[*non_leaf_id].$get_padding_fn(parent_width);

            // If the children are larger than the parents preferred max-width or smaller
            // than the parents min-width, adjust
            let child_width = match parent_width_metrics.$preferred_field {
                Between(min, max) => {
                    if children_flex_basis > (max - parent_padding)  {
                        max
                    } else if children_flex_basis < (min + parent_padding) {
                        min
                    } else {
                        children_flex_basis
                    }
                },
                EqualTo(exact) => exact - parent_padding,
                Unconstrained => children_flex_basis,
            };

            node_data[*non_leaf_id].min_inner_size_px = child_width;
        }

        // Now, the width of all elements should be filled,
        // but they aren't flex-growed or flex-shrinked yet
    }

    /// Go from the root down and flex_grow the children if needed - respects the `width`, `min_width` and `max_width` properties
    /// The layout step doesn't account for the min_width and max_width constraints, so we have to adjust them manually
    fn $apply_flex_grow_fn_name(
        node_data: &mut NodeDataContainer<$struct_name>,
        node_hierarchy: &NodeHierarchy,
        arena_data: &NodeDataContainer<RectLayout>,
        parent_ids_sorted_by_depth: &[(usize, NodeId)],
        root_width: f32)
    {
        /// Does the actual width layout, respects the `width`, `min_width` and `max_width`
        /// properties as well as the `flex_grow` factor. `flex_shrink` currently does nothing.
        fn distribute_space_along_main_axis(
            node_id: &NodeId,
            node_hierarchy: &NodeHierarchy,
            arena_data: &NodeDataContainer<RectLayout>,
            width_calculated_arena: &mut NodeDataContainer<$struct_name>,
            positioned_node_stack: &[NodeId])
        {
            // The inner space of the parent node, without the padding
            let mut parent_node_inner_width = {
                let parent_node = &width_calculated_arena[*node_id];
                let parent_parent_width = node_hierarchy[*node_id].parent.and_then(|p| width_calculated_arena[p].$preferred_field.min_needed_space()).unwrap_or(0.0);
                parent_node.min_inner_size_px + parent_node.flex_grow_px - parent_node.$get_padding_fn(parent_parent_width)
            };

            // 1. Set all child elements that have an exact width to that width, record their violations
            //    and add their violation to the leftover horizontal space.
            // let mut horizontal_space_from_fixed_width_items = 0.0;
            let mut horizontal_space_taken_up_by_fixed_width_items = 0.0;

            {
                // Vec<(NodeId, PreferredWidth)>
                let exact_width_childs = node_id
                        .children(node_hierarchy)
                        .filter_map(|id| if let WhConstraint::EqualTo(exact) = width_calculated_arena[id].$preferred_field {
                            Some((id, exact))
                        } else {
                            None
                        })
                        .collect::<Vec<(NodeId, f32)>>();

                for (exact_width_child_id, exact_width) in exact_width_childs {

                    // If this child node is `position: absolute`, it doesn't take any space away from
                    // its siblings, since it is taken out of the regular content flow
                    if arena_data[exact_width_child_id].position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Absolute {
                        horizontal_space_taken_up_by_fixed_width_items += exact_width;
                    }

                    // so that node.min_inner_size_px + node.flex_grow_px = exact_width
                    width_calculated_arena[exact_width_child_id].flex_grow_px =
                        exact_width - width_calculated_arena[exact_width_child_id].min_inner_size_px;
                }
            }

            // The fixed-width items are now considered solved, so subtract them out of the width of the parent.
            parent_node_inner_width -= horizontal_space_taken_up_by_fixed_width_items;

            // Now we can be sure that if we write #x { width: 500px; } that it will actually be 500px large
            // and not be influenced by flex in any way.

            // 2. Set all items to their minimum width. Record how much space is gained by doing so.
            let mut horizontal_space_taken_up_by_variable_items = 0.0;

            use azul_core::FastHashSet;

            let mut variable_width_childs = node_id
                .children(node_hierarchy)
                .filter(|id| !width_calculated_arena[*id].$preferred_field.is_fixed_constraint())
                .collect::<FastHashSet<NodeId>>();

            let mut absolute_variable_width_nodes = Vec::new();

            for variable_child_id in &variable_width_childs {

                if arena_data[*variable_child_id].position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Absolute {

                    let min_width = width_calculated_arena[*variable_child_id].$preferred_field.min_needed_space().unwrap_or(0.0);

                    horizontal_space_taken_up_by_variable_items += min_width;

                    // so that node.min_inner_size_px + node.flex_grow_px = min_width
                    width_calculated_arena[*variable_child_id].flex_grow_px =
                        min_width - width_calculated_arena[*variable_child_id].min_inner_size_px;

                } else {

                    // `position: absolute` items don't take space away from their siblings, rather
                    // they take the minimum needed space by their content

                    let root_id = NodeId::new(0);
                    let nearest_relative_parent_node = positioned_node_stack.get(positioned_node_stack.len() - 1).unwrap_or(&root_id);
                    let relative_parent_width = {
                        let relative_parent_node = &width_calculated_arena[*nearest_relative_parent_node];
                        relative_parent_node.flex_grow_px + relative_parent_node.min_inner_size_px
                    };

                    // By default, absolute positioned elements take the width of their content
                    // let min_inner_width = width_calculated_arena[*variable_child_id].$preferred_field.min_needed_space().unwrap_or(0.0);

                    // The absolute positioned node might have a max-width constraint, which has a
                    // higher precedence than `top, bottom, left, right`.
                    let max_space_current_node = match width_calculated_arena[*variable_child_id].$preferred_field {
                        WhConstraint::EqualTo(e) => e,
                        WhConstraint::Between(min, max) => {
                            if relative_parent_width > min {
                                if relative_parent_width < max {
                                    relative_parent_width
                                } else {
                                    max
                                }
                            } else {
                                min
                            }
                        },
                        WhConstraint::Unconstrained => relative_parent_width,
                    };

                    // so that node.min_inner_size_px + node.flex_grow_px = max_space_current_node
                    width_calculated_arena[*variable_child_id].flex_grow_px =
                        max_space_current_node - width_calculated_arena[*variable_child_id].min_inner_size_px;

                    absolute_variable_width_nodes.push(*variable_child_id);
                }

            }

            // Absolute positioned nodes aren't in the space-to-distribute set
            for absolute_node in absolute_variable_width_nodes {
                variable_width_childs.remove(&absolute_node);
            }

            // This satisfies the `width` and `min_width` constraints. However, we still need to worry about
            // the `max_width` and unconstrained children.
            //
            // By setting the items to their minimum size, we've gained some space that we now need to distribute
            // according to the flex_grow values
            parent_node_inner_width -= horizontal_space_taken_up_by_variable_items;

            let mut total_horizontal_space_available = parent_node_inner_width;
            let mut max_width_violations = Vec::new();

            loop {

                if total_horizontal_space_available <= 0.0 || variable_width_childs.is_empty() {
                    break;
                }

                // In order to apply flex-grow correctly, we need the sum of
                // the flex-grow factors of all the variable-width children
                //
                // NOTE: variable_width_childs can change its length, have to recalculate every loop!
                let children_combined_flex_grow: f32 = variable_width_childs
                    .iter()
                    .map(|child_id|
                            // Prevent flex-grow and flex-shrink to be less than 1
                            arena_data[*child_id].flex_grow
                                .and_then(|g| g.get_property().copied())
                                .and_then(|grow| Some(grow.inner.get().max(1.0)))
                                .unwrap_or(DEFAULT_FLEX_GROW_FACTOR))
                    .sum();

                // Grow all variable children by the same amount.
                for variable_child_id in &variable_width_childs {

                    let flex_grow = arena_data[*variable_child_id].flex_grow
                        .and_then(|g| g.get_property().copied())
                        .and_then(|grow| Some(grow.inner.get().max(1.0)))
                        .unwrap_or(DEFAULT_FLEX_GROW_FACTOR);

                    let added_space_for_one_child = total_horizontal_space_available * (flex_grow / children_combined_flex_grow);

                    let current_width_of_child = width_calculated_arena[*variable_child_id].min_inner_size_px +
                                                 width_calculated_arena[*variable_child_id].flex_grow_px;

                    if let Some(max_width) = width_calculated_arena[*variable_child_id].$preferred_field.max_available_space() {
                        if (current_width_of_child + added_space_for_one_child) > max_width {
                            // so that node.min_inner_size_px + node.flex_grow_px = max_width
                            width_calculated_arena[*variable_child_id].flex_grow_px =
                                max_width - width_calculated_arena[*variable_child_id].min_inner_size_px;

                            max_width_violations.push(*variable_child_id);
                        } else {
                            // so that node.min_inner_size_px + node.flex_grow_px = added_space_for_one_child
                            width_calculated_arena[*variable_child_id].flex_grow_px =
                                added_space_for_one_child - width_calculated_arena[*variable_child_id].min_inner_size_px;
                        }
                    } else {
                        // so that node.min_inner_size_px + node.flex_grow_px = added_space_for_one_child
                        width_calculated_arena[*variable_child_id].flex_grow_px =
                            added_space_for_one_child - width_calculated_arena[*variable_child_id].min_inner_size_px;
                    }
                }

                // If we haven't violated any max_width constraints, then we have
                // added all remaining widths and thereby successfully solved the layout
                if max_width_violations.is_empty() {
                    break;
                } else {
                    // Nodes that were violated can't grow anymore in the next iteration,
                    // so we remove them from the solution and consider them "solved".
                    // Their amount of violation then gets distributed across the remaining
                    // items in the next iteration.
                    for solved_node_id in max_width_violations.drain(..) {

                        // Since the node now gets removed, it doesn't contribute to the pool anymore
                        total_horizontal_space_available -=
                            width_calculated_arena[solved_node_id].min_inner_size_px +
                            width_calculated_arena[solved_node_id].flex_grow_px;

                        variable_width_childs.remove(&solved_node_id);
                    }
                }
            }
        }

        fn distribute_space_along_cross_axis(
            node_id: &NodeId,
            node_hierarchy: &NodeHierarchy,
            arena_data: &NodeDataContainer<RectLayout>,
            width_calculated_arena: &mut NodeDataContainer<$struct_name>,
            positioned_node_stack: &[NodeId])
        {
            // The inner space of the parent node, without the padding
            let parent_node_inner_width = {
                let parent_node = &width_calculated_arena[*node_id];
                let parent_parent_width = node_hierarchy[*node_id].parent.and_then(|p| width_calculated_arena[p].$preferred_field.min_needed_space()).unwrap_or(0.0);
                parent_node.min_inner_size_px + parent_node.flex_grow_px - parent_node.$get_padding_fn(parent_parent_width)
            };

            let last_relative_node_width = {
                let zero_node = NodeId::new(0);
                let last_relative_node_id = positioned_node_stack.get(positioned_node_stack.len() - 1).unwrap_or(&zero_node);
                let last_relative_node = &width_calculated_arena[*last_relative_node_id];
                let last_relative_node_parent_width = node_hierarchy[*last_relative_node_id].parent.and_then(|p| width_calculated_arena[p].$preferred_field.min_needed_space()).unwrap_or(0.0);
                last_relative_node.min_inner_size_px + last_relative_node.flex_grow_px - last_relative_node.$get_padding_fn(last_relative_node_parent_width)
            };

            for child_id in node_id.children(node_hierarchy) {

                let parent_node_inner_width = if arena_data[child_id].position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Absolute {
                    parent_node_inner_width
                } else {
                    last_relative_node_width
                };

                let preferred_width = {
                    let min_width = width_calculated_arena[child_id].$preferred_field.min_needed_space().unwrap_or(0.0);
                    // In this case we want to overflow if the min width of the cross axis
                    if min_width > parent_node_inner_width {
                        min_width
                    } else {
                        if let Some(max_width) = width_calculated_arena[child_id].$preferred_field.max_available_space() {
                            if max_width > parent_node_inner_width {
                                parent_node_inner_width
                            } else {
                                max_width
                            }
                        } else {
                            parent_node_inner_width
                        }
                    }
                };

                // so that node.min_inner_size_px + node.flex_grow_px = preferred_width
                width_calculated_arena[child_id].flex_grow_px =
                    preferred_width - width_calculated_arena[child_id].min_inner_size_px;
            }
        }

        debug_assert!(node_data[NodeId::new(0)].flex_grow_px == 0.0);

        // Set the window width on the root node (since there is only one root node, we can
        // calculate the `flex_grow_px` directly)
        //
        // Usually `top_level_flex_basis` is NOT 0.0, rather it's the sum of all widths in the DOM,
        // i.e. the sum of the whole DOM tree
        let top_level_flex_basis = node_data[NodeId::new(0)].min_inner_size_px;

        // The root node can still have some sort of max-width attached, so we need to check for that
        let root_preferred_width = if let Some(max_width) = node_data[NodeId::new(0)].$preferred_field.max_available_space() {
            if root_width > max_width { max_width } else { root_width }
        } else {
            root_width
        };

        node_data[NodeId::new(0)].flex_grow_px = root_preferred_width - top_level_flex_basis;

        // Keep track of the nearest relative or absolute positioned element
        let mut positioned_node_stack = vec![NodeId::new(0)];

        for (_node_depth, parent_id) in parent_ids_sorted_by_depth {

            use azul_css::{LayoutAxis, LayoutPosition};

            let parent_is_positioned = arena_data[*parent_id].position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Static;
            if parent_is_positioned {
                positioned_node_stack.push(*parent_id);
            }

            if arena_data[*parent_id].direction.unwrap_or_default().get_property_or_default().unwrap_or_default().get_axis() == LayoutAxis::$main_axis {
                distribute_space_along_main_axis(parent_id, node_hierarchy, arena_data, node_data, &positioned_node_stack);
            } else {
                distribute_space_along_cross_axis(parent_id, node_hierarchy, arena_data, node_data, &positioned_node_stack);
            }

            if parent_is_positioned {
                positioned_node_stack.pop();
            }
        }
    }

    /// Returns the sum of the flex-basis of the current nodes' children
    #[must_use]
    fn $sum_children_flex_basis_fn_name(
        node_data: &NodeDataContainer<$struct_name>,
        node_id: NodeId,
        node_hierarchy: &NodeHierarchy,
        display_arena: &NodeDataContainer<RectLayout>)
    -> f32
    {
        let parent_width = node_data[node_id].$preferred_field.max_available_space().unwrap_or(0.0);
        node_id
            .children(node_hierarchy)
            .filter(|child_node_id| display_arena[*child_node_id].position.and_then(|p| p.get_property().copied()) != Some(LayoutPosition::Absolute))
            .map(|child_node_id| node_data[child_node_id].$get_flex_basis(parent_width))
            .sum()
    }

)}

typed_arena!(
    WidthCalculatedRect,
    preferred_width,
    determine_preferred_width,
    get_horizontal_padding,
    get_flex_basis_horizontal,
    width_calculated_rect_arena_from_rect_layout_arena,
    bubble_preferred_widths_to_parents,
    width_calculated_rect_arena_apply_flex_grow,
    width_calculated_rect_arena_sum_children_flex_basis,
    Horizontal
);

typed_arena!(
    HeightCalculatedRect,
    preferred_height,
    determine_preferred_height,
    get_vertical_padding,
    get_flex_basis_vertical,
    height_calculated_rect_arena_from_rect_layout_arena,
    bubble_preferred_heights_to_parents,
    height_calculated_rect_arena_apply_flex_grow,
    height_calculated_rect_arena_sum_children_flex_basis,
    Vertical
);

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct WidthSolvedResult {
    pub min_width: f32,
    pub space_added: f32,
}

impl WidthSolvedResult {
    pub fn total(&self) -> f32 {
        self.min_width + self.space_added
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct HeightSolvedResult {
    pub min_height: f32,
    pub space_added: f32,
}

impl HeightSolvedResult {
    pub fn total(&self) -> f32 {
        self.min_height + self.space_added
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SolvedWidthLayout {
    pub solved_widths: NodeDataContainer<WidthSolvedResult>,
    pub layout_only_arena: NodeDataContainer<RectLayout>,
    pub non_leaf_nodes_sorted_by_depth: Vec<(usize, NodeId)>,
}

#[derive(Debug, Clone)]
pub(crate) struct SolvedHeightLayout {
    pub solved_heights: NodeDataContainer<HeightSolvedResult>,
}

/// Returns the solved widths of the items in a BTree form
pub(crate) fn solve_flex_layout_width(
    node_hierarchy: &NodeHierarchy,
    display_rectangles: &NodeDataContainer<DisplayRectangle>,
    preferred_widths: &NodeDataContainer<Option<f32>>,
    window_width: f32
) -> SolvedWidthLayout {
    let layout_only_arena = display_rectangles.transform(|node, _| node.layout);
    let mut width_calculated_arena = width_calculated_rect_arena_from_rect_layout_arena(&layout_only_arena, preferred_widths, node_hierarchy);
    let non_leaf_nodes_sorted_by_depth = node_hierarchy.get_parents_sorted_by_depth();
    bubble_preferred_widths_to_parents(&mut width_calculated_arena, node_hierarchy, &layout_only_arena, &non_leaf_nodes_sorted_by_depth);
    width_calculated_rect_arena_apply_flex_grow(&mut width_calculated_arena, node_hierarchy, &layout_only_arena, &non_leaf_nodes_sorted_by_depth, window_width);
    let solved_widths = width_calculated_arena.transform(|node, _| node.solved_result());
    SolvedWidthLayout { solved_widths , layout_only_arena, non_leaf_nodes_sorted_by_depth }
}

/// Returns the solved height of the items in a BTree form
pub(crate) fn solve_flex_layout_height(
    node_hierarchy: &NodeHierarchy,
    solved_widths: &SolvedWidthLayout,
    preferred_heights: &NodeDataContainer<Option<f32>>,
    window_height: f32
) -> SolvedHeightLayout {
    let SolvedWidthLayout { layout_only_arena, .. } = solved_widths;
    let mut height_calculated_arena = height_calculated_rect_arena_from_rect_layout_arena(&layout_only_arena, preferred_heights, node_hierarchy);
    bubble_preferred_heights_to_parents(&mut height_calculated_arena, node_hierarchy, &layout_only_arena, &solved_widths.non_leaf_nodes_sorted_by_depth);
    height_calculated_rect_arena_apply_flex_grow(&mut height_calculated_arena, node_hierarchy, &layout_only_arena, &solved_widths.non_leaf_nodes_sorted_by_depth, window_height);
    let solved_heights = height_calculated_arena.transform(|node, _| node.solved_result());
    SolvedHeightLayout { solved_heights }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub(crate) struct HorizontalSolvedPosition(pub f32);

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub(crate) struct VerticalSolvedPosition(pub f32);

macro_rules! get_position {
($fn_name:ident,
 $width_layout:ident,
 $height_solved_position:ident,
 $solved_widths_field:ident,
 $min_width:ident,
 $left:ident,
 $right:ident,
 $margin_left:ident,
 $margin_right:ident,
 $padding_left:ident,
 $padding_right:ident,
 $axis:ident
) => (

/// Traverses along the DOM and solve for the X or Y position
fn $fn_name(
    node_hierarchy: &NodeHierarchy,
    node_data: &NodeDataContainer<RectLayout>,
    non_leaf_nodes: &[(usize, NodeId)],
    solved_widths: &$width_layout)
-> NodeDataContainer<$height_solved_position>
{
    fn determine_child_x_absolute(
        child_id: NodeId,
        positioned_node_stack: &[NodeId],
        arena_data: &NodeDataContainer<RectLayout>,
        arena_solved_data: &mut NodeDataContainer<$height_solved_position>,
        solved_widths: &$width_layout,
        node_hierarchy: &NodeHierarchy,
    ) {
        let child_width_with_padding = {
            let child_node = &solved_widths.$solved_widths_field[child_id];
            child_node.$min_width + child_node.space_added
        };

        let child_node = &arena_data[child_id];
        let child_node_parent_width = node_hierarchy[child_id].parent.map(|p| solved_widths.$solved_widths_field[p].total()).unwrap_or(0.0);
        let child_margin_left = child_node.$margin_left.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);
        let child_margin_right = child_node.$margin_right.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);

        let zero_node = NodeId::new(0);
        let last_relative_node_id = positioned_node_stack.get(positioned_node_stack.len() - 1).unwrap_or(&zero_node);

        let last_relative_node = arena_data[*last_relative_node_id];
        let last_relative_padding_left = last_relative_node.$padding_left.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);
        let last_relative_padding_right = last_relative_node.$padding_right.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);

        let last_relative_node_x = arena_solved_data[*last_relative_node_id].0 + last_relative_padding_left;
        let last_relative_node_inner_width = {
            let last_relative_node = &solved_widths.$solved_widths_field[*last_relative_node_id];
            last_relative_node.$min_width + last_relative_node.space_added - (last_relative_padding_left + last_relative_padding_right)
        };

        let child_left = &arena_data[child_id].$left.and_then(|s| Some(s.get_property()?.inner.to_pixels(child_node_parent_width)));
        let child_right = &arena_data[child_id].$right.and_then(|s| Some(s.get_property()?.inner.to_pixels(child_node_parent_width)));

        if let Some(child_right) = child_right {
            // align right / bottom of last relative parent
            arena_solved_data[child_id].0 =
                last_relative_node_x
                + last_relative_node_inner_width
                - child_width_with_padding
                - child_margin_right
                - child_right;
        } else {
            // align left / top of last relative parent
            arena_solved_data[child_id].0 =
                last_relative_node_x
                + child_margin_left
                + child_left.unwrap_or(0.0);
        }
    }

    fn determine_child_x_along_main_axis(
        main_axis_alignment: LayoutJustifyContent,
        arena_data: &NodeDataContainer<RectLayout>,
        arena_solved_data: &mut NodeDataContainer<$height_solved_position>,
        solved_widths: &$width_layout,
        child_id: NodeId,
        parent_x_position: f32,
        parent_inner_width: f32,
        sum_x_of_children_so_far: &mut f32,
        positioned_node_stack: &[NodeId],
        node_hierarchy: &NodeHierarchy
    ) {
        use azul_css::LayoutJustifyContent::*;

        let child_width_with_padding = {
            let child_node = &solved_widths.$solved_widths_field[child_id];
            child_node.$min_width + child_node.space_added
        };

        // width: increase X according to the main axis, Y according to the cross_axis
        let child_node = &arena_data[child_id];
        let child_node_parent_width = node_hierarchy[child_id].parent.map(|p| solved_widths.$solved_widths_field[p].total()).unwrap_or(0.0);
        let child_margin_left = child_node.$margin_left.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);
        let child_margin_right = child_node.$margin_right.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);

        if child_node.position.unwrap_or_default().get_property_or_default().unwrap_or_default() == LayoutPosition::Absolute {
            determine_child_x_absolute(
                child_id,
                positioned_node_stack,
                arena_data,
                arena_solved_data,
                solved_widths,
                node_hierarchy,
            );
        } else {
            // X position of the top left corner
            // WARNING: End has to be added after all children!
            let x_of_top_left_corner = match main_axis_alignment {
                Start | End => {
                    parent_x_position + *sum_x_of_children_so_far + child_margin_left
                },
                Center => {
                    parent_x_position
                    + ((parent_inner_width / 2.0)
                    - ((*sum_x_of_children_so_far + child_margin_right + child_width_with_padding) / 2.0))
                },
                SpaceBetween => {
                    parent_x_position // TODO!
                },
                SpaceAround => {
                    parent_x_position // TODO!
                },
                SpaceEvenly => {
                    parent_x_position // TODO!
                },
            };

            arena_solved_data[child_id].0 = x_of_top_left_corner;
            *sum_x_of_children_so_far += child_margin_right + child_width_with_padding + child_margin_left;
        }
    }

    fn determine_child_x_along_cross_axis(
        arena_data: &NodeDataContainer<RectLayout>,
        solved_widths: &$width_layout,
        child_id: NodeId,
        positioned_node_stack: &[NodeId],
        arena_solved_data: &mut NodeDataContainer<$height_solved_position>,
        parent_x_position: f32,
        node_hierarchy: &NodeHierarchy)
    {
        let child_node = &arena_data[child_id];
        let child_node_parent_width = node_hierarchy[child_id].parent.map(|p| solved_widths.$solved_widths_field[p].total()).unwrap_or(0.0);
        let child_margin_left = child_node.$margin_left.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);

        if child_node.position.unwrap_or_default().get_property_or_default().unwrap_or_default() == LayoutPosition::Absolute {
            determine_child_x_absolute(
                child_id,
                positioned_node_stack,
                arena_data,
                arena_solved_data,
                solved_widths,
                node_hierarchy,
            );
        } else {
            arena_solved_data[child_id].0 = parent_x_position + child_margin_left;
        }
    }

    use azul_css::{LayoutAxis, LayoutJustifyContent};

    let mut arena_solved_data = NodeDataContainer::new(vec![$height_solved_position(0.0); node_data.len()]);

    // Stack of the positioned nodes (nearest relative or absolute positioned node)
    let mut positioned_node_stack = vec![NodeId::new(0)];

    for (_node_depth, parent_id) in non_leaf_nodes {

        let parent_node = node_data[*parent_id];
        let parent_parent_width = node_hierarchy[*parent_id].parent.map(|p| solved_widths.$solved_widths_field[p].total()).unwrap_or(0.0);
        let parent_padding_left = parent_node.$padding_left.and_then(|x| Some(x.get_property()?.inner.to_pixels(parent_parent_width))).unwrap_or(0.0);
        let parent_padding_right = parent_node.$padding_right.and_then(|x| Some(x.get_property()?.inner.to_pixels(parent_parent_width))).unwrap_or(0.0);
        let parent_x_position = arena_solved_data[*parent_id].0 + parent_padding_left;
        let parent_direction = parent_node.direction.and_then(|g| g.get_property_or_default()).unwrap_or_default();

        // Push nearest relative or absolute positioned element
        let parent_is_positioned = parent_node.position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Static;
        if parent_is_positioned {
            positioned_node_stack.push(*parent_id);
        }

        let parent_inner_width = {
            let parent_node = &solved_widths.$solved_widths_field[*parent_id];
            parent_node.$min_width + parent_node.space_added - (parent_padding_left + parent_padding_right)
        };

        if parent_direction.get_axis() == LayoutAxis::$axis {
            // Along main axis: Take X of parent
            let main_axis_alignment = node_data[*parent_id].justify_content.unwrap_or_default().get_property_or_default().unwrap_or_default();
            let mut sum_x_of_children_so_far = 0.0;

            if parent_direction.is_reverse() {
                for child_id in parent_id.reverse_children(node_hierarchy) {
                    determine_child_x_along_main_axis(
                        main_axis_alignment,
                        &node_data,
                        &mut arena_solved_data,
                        solved_widths,
                        child_id,
                        parent_x_position,
                        parent_inner_width,
                        &mut sum_x_of_children_so_far,
                        &positioned_node_stack,
                        node_hierarchy,
                    );
                }
            } else {
                for child_id in parent_id.children(node_hierarchy) {
                    determine_child_x_along_main_axis(
                        main_axis_alignment,
                        &node_data,
                        &mut arena_solved_data,
                        solved_widths,
                        child_id,
                        parent_x_position,
                        parent_inner_width,
                        &mut sum_x_of_children_so_far,
                        &positioned_node_stack,
                        node_hierarchy,
                    );
                }
            }

            // If the direction is `flex-end`, we can't add the X position during the iteration,
            // so we have to "add" the diff to the parent_inner_width at the end
            let should_align_towards_end =
                (!parent_direction.is_reverse() && main_axis_alignment == LayoutJustifyContent::End) ||
                (parent_direction.is_reverse() && main_axis_alignment == LayoutJustifyContent::Start);

            if should_align_towards_end {
                let diff = parent_inner_width - sum_x_of_children_so_far;
                for child_id in parent_id.children(node_hierarchy).filter(|ch| {
                    node_data[*ch].position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Absolute
                }) {
                    arena_solved_data[child_id].0 += diff;
                }
            }

        } else {
            // Along cross axis: Increase X with width of current element

            if parent_direction.is_reverse() {
                for child_id in parent_id.reverse_children(node_hierarchy) {
                    determine_child_x_along_cross_axis(
                        node_data,
                        solved_widths,
                        child_id,
                        &positioned_node_stack,
                        &mut arena_solved_data,
                        parent_x_position,
                        node_hierarchy,
                    );
                }
            } else {
                for child_id in parent_id.children(node_hierarchy) {
                    determine_child_x_along_cross_axis(
                        node_data,
                        solved_widths,
                        child_id,
                        &positioned_node_stack,
                        &mut arena_solved_data,
                        parent_x_position,
                        node_hierarchy,
                    );
                }
            }
        }

        if parent_is_positioned {
            positioned_node_stack.pop();
        }

    }

    arena_solved_data
}

)}

fn get_x_positions(
    solved_widths: &SolvedWidthLayout,
    node_hierarchy: &NodeHierarchy,
    origin: LayoutPoint,
) -> NodeDataContainer<HorizontalSolvedPosition>
{
    get_position!(
        get_pos_x,
        SolvedWidthLayout,
        HorizontalSolvedPosition,
        solved_widths,
        min_width,
        left,
        right,
        margin_left,
        margin_right,
        padding_left,
        padding_right,
        Horizontal
    );
    let mut arena = get_pos_x(node_hierarchy, &solved_widths.layout_only_arena, &solved_widths.non_leaf_nodes_sorted_by_depth, solved_widths);

    // Add the origin on top of the position
    let x = origin.x as f32;
    if x > 0.5 || x < -0.5 {
        for item in &mut arena.internal {
            item.0 += x;
        }
    }
    arena
}

fn get_y_positions(
    solved_heights: &SolvedHeightLayout,
    solved_widths: &SolvedWidthLayout,
    node_hierarchy: &NodeHierarchy,
    origin: LayoutPoint
) -> NodeDataContainer<VerticalSolvedPosition>
{
    get_position!(
        get_pos_y,
        SolvedHeightLayout,
        VerticalSolvedPosition,
        solved_heights,
        min_height,
        top,
        bottom,
        margin_top,
        margin_bottom,
        padding_top,
        padding_bottom,
        Vertical
    );
    let mut arena = get_pos_y(node_hierarchy, &solved_widths.layout_only_arena, &solved_widths.non_leaf_nodes_sorted_by_depth, solved_heights);

    // Add the origin on top of the position
    let y = origin.y as f32;
    if y > 0.5  || y < -0.5 {
        for item in &mut arena.internal {
            item.0 += y;
        }
    }
    arena
}

/// Returns the preferred width, for example for an image, that would be the
/// original width (an image always wants to take up the original space)
fn get_content_width(
    pipeline_id: &PipelineId,
    node_id: &NodeId,
    node_type: &NodeType,
    app_resources: &AppResources,
    positioned_words: &BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
) -> Option<f32> {
    use azul_core::dom::NodeType::*;
    match node_type {
        Image(image_id) => app_resources.get_image_info(pipeline_id, image_id).map(|info| info.descriptor.dimensions.0 as f32),
        Label(_) | Text(_) => positioned_words.get(node_id).map(|(pos, _)| pos.content_size.width),
        _ => None,
    }
}

fn get_content_height(
    pipeline_id: &PipelineId,
    node_id: &NodeId,
    node_type: &NodeType,
    app_resources: &AppResources,
    positioned_words: &BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    div_width: f32,
) -> Option<f32> {
    use azul_core::dom::NodeType::*;
    match &node_type {
        Image(i) => {
            let image_size = &app_resources.get_image_info(pipeline_id, i)?.descriptor.dimensions;
            Some(div_width * (image_size.0 as f32 / image_size.1 as f32))
        },
        Label(_) | Text(_) => {
            positioned_words.get(node_id).map(|(pos, _)| pos.content_size.height)
        }
        _ => None,
    }
}
/*
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PreferredHeight {
    Image { original_dimensions: (f32, f32), current_height: f32 },
    Text(WordPositions)
}

impl PreferredHeight {
    /// Returns the preferred size of the div content.
    /// Note that this can be larger than the actual div content!
    pub fn get_content_size(&self) -> f32 {
        use self::PreferredHeight::*;
        match self {
            Image { current_height, .. } => *current_height,
            Text(word_positions) => word_positions.content_size.height,
        }
    }
}
*/
#[inline]
fn get_layout_positions(display_rects: &NodeDataContainer<RectLayout>) -> NodeDataContainer<LayoutPosition> {
    display_rects.transform(|node, node_id| node.position.unwrap_or_default().get_property_or_default().unwrap_or_default())
}

fn get_overflow(layout: &RectLayout, parent_rect: &LayoutRect, children_sum_rect: &Option<LayoutRect>) -> OverflowInfo {

    use azul_css::Overflow;
    use azul_core::ui_solver::DirectionalOverflowInfo;

    let overflow_x = layout.overflow_x.unwrap_or_default().get_property_or_default().unwrap_or_default();
    let overflow_y = layout.overflow_y.unwrap_or_default().get_property_or_default().unwrap_or_default();

    match children_sum_rect {
        Some(children_sum_rect) => {
            let overflow_x_amount = (parent_rect.size.width + parent_rect.origin.x) - (children_sum_rect.origin.x + children_sum_rect.size.width);
            let overflow_y_amount = (parent_rect.size.height + parent_rect.origin.y) - (children_sum_rect.origin.y + children_sum_rect.size.height);
            OverflowInfo {
                overflow_x: match overflow_x {
                    Overflow::Scroll => DirectionalOverflowInfo::Scroll { amount: Some(overflow_x_amount) },
                    Overflow::Auto => DirectionalOverflowInfo::Auto { amount: Some(overflow_x_amount) },
                    Overflow::Hidden => DirectionalOverflowInfo::Hidden { amount: Some(overflow_x_amount) },
                    Overflow::Visible => DirectionalOverflowInfo::Visible { amount: Some(overflow_x_amount) },
                },
                overflow_y: match overflow_y {
                    Overflow::Scroll => DirectionalOverflowInfo::Scroll { amount: Some(overflow_y_amount) },
                    Overflow::Auto => DirectionalOverflowInfo::Auto { amount: Some(overflow_y_amount) },
                    Overflow::Hidden => DirectionalOverflowInfo::Hidden { amount: Some(overflow_y_amount) },
                    Overflow::Visible => DirectionalOverflowInfo::Visible { amount: Some(overflow_y_amount) },
                }
            }
        },
        None => {
            OverflowInfo {
                overflow_x: match overflow_x {
                    Overflow::Scroll => DirectionalOverflowInfo::Scroll { amount: None },
                    Overflow::Auto => DirectionalOverflowInfo::Auto { amount: None },
                    Overflow::Hidden => DirectionalOverflowInfo::Hidden { amount: None },
                    Overflow::Visible => DirectionalOverflowInfo::Visible { amount: None },
                },
                overflow_y: match overflow_y {
                    Overflow::Scroll => DirectionalOverflowInfo::Scroll { amount: None },
                    Overflow::Auto => DirectionalOverflowInfo::Auto { amount: None },
                    Overflow::Hidden => DirectionalOverflowInfo::Hidden { amount: None },
                    Overflow::Visible => DirectionalOverflowInfo::Visible { amount: None },
                }
            }
        }
    }
}

macro_rules! get_resolved_offsets {(
    $fn_name:ident,
    $left:ident,
    $top:ident,
    $bottom:ident,
    $right:ident
) => (
    fn $fn_name(layout: &RectLayout, scale_x: f32, scale_y: f32) -> ResolvedOffsets {
        ResolvedOffsets {
            left: layout.$left.unwrap_or_default().get_property_or_default().unwrap_or_default().inner.to_pixels(scale_x),
            top: layout.$top.unwrap_or_default().get_property_or_default().unwrap_or_default().inner.to_pixels(scale_y),
            bottom: layout.$bottom.unwrap_or_default().get_property_or_default().unwrap_or_default().inner.to_pixels(scale_y),
            right: layout.$right.unwrap_or_default().get_property_or_default().unwrap_or_default().inner.to_pixels(scale_x),
        }
    }
)}

get_resolved_offsets!(get_margin, margin_left, margin_top, margin_bottom, margin_right);
get_resolved_offsets!(get_padding, padding_left, padding_top, padding_bottom, padding_right);
get_resolved_offsets!(get_border_widths, border_left_width, border_top_width, border_bottom_width, border_right_width);

/// At this point in time, all font keys, image keys, etc. have
/// to be already submitted in the RenderApi!
pub fn do_the_layout(
    node_hierarchy: &NodeHierarchy,
    node_data: &NodeDataContainer<NodeData>,
    display_rects: &NodeDataContainer<DisplayRectangle>,
    app_resources: &AppResources,
    pipeline_id: &PipelineId,
    bounds: LayoutRect,
) -> LayoutResult {

    use azul_core::ui_solver::PositionInfo;
    use azul_text_layout::text_layout::get_layouted_glyphs;

    let rect_size = bounds.size;
    let rect_offset = bounds.origin;

    // TODO: Filter all inline text blocks: inline blocks + their padding + margin
    // The NodeId has to be the **next** NodeId (the next sibling after the inline element)
    // let mut inline_text_blocks = BTreeMap::<NodeId, InlineText>::new();

    let content_width_pre = node_data.transform(|node, node_id| None);
    let solved_widths = solve_flex_layout_width(
        node_hierarchy,
        &display_rects,
        &content_width_pre,
        rect_size.width as f32,
    );

    // Break all strings into words and / or resolve the TextIds
    let word_cache = create_word_cache(app_resources, node_data);
    // Scale the words to the correct size - TODO: Cache this in the app_resources!
    let scaled_words = create_scaled_words(pipeline_id, app_resources, &word_cache, display_rects);
    // Layout all words as if there was no max-width constraint (to get the texts "content width").
    let word_positions_no_max_width = create_word_positions(pipeline_id, &word_cache, &scaled_words, display_rects, &solved_widths);

    // Determine the preferred **content** width
    // let content_widths = node_data.transform(|node, node_id| {
    //     get_content_width(pipeline_id, &node_id, &node.get_node_type(), app_resources, &word_positions_no_max_width)
    // });

    // // Layout the words again, this time with the proper width constraints!
    // let proper_max_widths = solved_widths.solved_widths.linear_iter().map(|node_id| {
    //     (node_id, TextSizePx(solved_widths.solved_widths[node_id].total()))
    // }).collect();

    // let word_positions_with_max_width = create_word_positions(&word_cache, &scaled_words, display_rects, &proper_max_widths, &inline_text_blocks);

    // Get the content height of the content
    // let content_heights = node_data.transform(|node, node_id| {
    //     let div_width = solved_widths.solved_widths[node_id].total();
    //     get_content_height(pipeline_id, &node_id, &node.get_node_type(), app_resources, &word_positions_no_max_width, div_width)
    // });

    let content_heights_pre = node_data.transform(|node, node_id| None);

    // TODO: The content height is not the final height!
    let solved_heights = solve_flex_layout_height(
        node_hierarchy,
        &solved_widths,
        &content_heights_pre,
        rect_size.height as f32,
    );

    let x_positions = get_x_positions(&solved_widths, node_hierarchy, rect_offset.clone());
    let y_positions = get_y_positions(&solved_heights, &solved_widths, node_hierarchy, rect_offset);
    let position_info = get_layout_positions(&solved_widths.layout_only_arena);
    let mut glyph_map = BTreeMap::new();

    let node_depths = &solved_widths.non_leaf_nodes_sorted_by_depth;
    let mut positioned_rects = NodeDataContainer { internal: vec![PositionedRectangle::default(); node_data.len()].into() };
    let mut positioned_node_stack = vec![NodeId::new(0)];

    // create the final positioned rectangles
    for (_depth, parent_node_id) in node_depths {

        let parent_rect_layout = &solved_widths.layout_only_arena[*parent_node_id];
        let parent_position = position_info[*parent_node_id];
        let width = solved_widths.solved_widths[*parent_node_id];
        let height = solved_heights.solved_heights[*parent_node_id];
        let x_pos = x_positions[*parent_node_id].0;
        let y_pos = y_positions[*parent_node_id].0;

        let parent_parent_node_id = node_hierarchy[*parent_node_id].parent.unwrap_or(NodeId::new(0));
        let parent_x_pos = x_positions[parent_parent_node_id].0;
        let parent_y_pos = y_positions[parent_parent_node_id].0;
        let parent_parent_width = solved_widths.solved_widths[parent_parent_node_id];
        let parent_parent_height = solved_heights.solved_heights[parent_parent_node_id];

        let last_positioned_item_node_id = positioned_node_stack.last().map(|l| *l).unwrap_or(NodeId::new(0));
        let last_positioned_item_x_pos = x_positions[last_positioned_item_node_id].0;
        let last_positioned_item_y_pos = y_positions[last_positioned_item_node_id].0;
        let parent_position_info = match parent_position {
            LayoutPosition::Static => PositionInfo::Static {
                // calculate relative to parent
                x_offset: x_pos - parent_x_pos,
                y_offset: y_pos - parent_y_pos,
                static_x_offset: x_pos,
                static_y_offset: y_pos,
            },
            LayoutPosition::Relative => PositionInfo::Relative {
                // calculate relative to parent
                x_offset: x_pos - parent_x_pos,
                y_offset: y_pos - parent_y_pos,
                static_x_offset: x_pos,
                static_y_offset: y_pos,
            },
            LayoutPosition::Absolute => PositionInfo::Absolute {
                // calculate relative to last positioned item
                x_offset: x_pos - last_positioned_item_x_pos,
                y_offset: y_pos - last_positioned_item_y_pos,
                static_x_offset: x_pos,
                static_y_offset: y_pos,
            },
            LayoutPosition::Fixed => PositionInfo::Fixed {
                // relative to screen, already done
                x_offset: x_pos,
                y_offset: y_pos,
                static_x_offset: x_pos,
                static_y_offset: y_pos,
            },
        };
        let parent_size = LayoutSize::new(width.total(), height.total());
        let parent_padding = get_padding(&parent_rect_layout, parent_parent_width.total(), parent_parent_height.total());
        let parent_margin = get_margin(&parent_rect_layout, parent_parent_width.total(), parent_parent_height.total());
        let parent_border_widths = get_border_widths(&parent_rect_layout, parent_parent_width.total(), parent_parent_height.total());
        let parent_parent_size = LayoutSize::new(parent_parent_width.total(), parent_parent_height.total());

        let parent_sum_rect = LayoutRect::new(LayoutPoint::new(x_pos, y_pos), parent_size);
        let mut children_sum_rects = Vec::new();

        // push positioned item and layout children
        if parent_position != LayoutPosition::Static {
            positioned_node_stack.push(*parent_node_id);
        }

        // set text, if any
        let parent_text = if let (Some(words), Some((scaled_words, _)), Some((word_positions, _))) = (word_cache.get(parent_node_id), scaled_words.get(parent_node_id), word_positions_no_max_width.get(parent_node_id)) {
            let mut inline_text_layout = InlineText { words, scaled_words }.get_text_layout(*pipeline_id, *parent_node_id, &word_positions.text_layout_options);
            let (horz_alignment, vert_alignment) = determine_text_alignment(&display_rects[*parent_node_id]);
            inline_text_layout.align_children_horizontal(horz_alignment);
            inline_text_layout.align_children_vertical_in_parent_bounds(&parent_parent_size, vert_alignment);
            let bounds = inline_text_layout.get_bounds();
            let glyphs = get_layouted_glyphs(word_positions, scaled_words, &inline_text_layout);
            glyph_map.insert(*parent_node_id, glyphs);
            Some((word_positions.text_layout_options.clone(), inline_text_layout, bounds))
        } else {
            None
        };

        for child_node_id in parent_node_id.children(&node_hierarchy) {

            // copy the width and height from the parent node
            let parent_width = width;
            let parent_height = height;
            let parent_x_pos = x_pos;
            let parent_y_pos = y_pos;

            let width = solved_widths.solved_widths[child_node_id];
            let height = solved_heights.solved_heights[child_node_id];
            let x_pos = x_positions[child_node_id].0;
            let y_pos = y_positions[child_node_id].0;
            let child_rect_layout = &solved_widths.layout_only_arena[child_node_id];
            let child_position = position_info[child_node_id];

            let child_position = match child_position {
                LayoutPosition::Static => PositionInfo::Static {
                    // calculate relative to parent
                    x_offset: x_pos - parent_x_pos,
                    y_offset: y_pos - parent_y_pos,
                    static_x_offset: x_pos,
                    static_y_offset: y_pos,
                },
                LayoutPosition::Relative => PositionInfo::Relative {
                    // calculate relative to parent
                    x_offset: x_pos - parent_x_pos,
                    y_offset: y_pos - parent_y_pos,
                    static_x_offset: x_pos,
                    static_y_offset: y_pos,
                },
                LayoutPosition::Absolute => PositionInfo::Absolute {
                    // calculate relative to last positioned item
                    x_offset: x_pos - last_positioned_item_x_pos,
                    y_offset: y_pos - last_positioned_item_y_pos,
                    static_x_offset: x_pos,
                    static_y_offset: y_pos,
                },
                LayoutPosition::Fixed => PositionInfo::Fixed {
                    // relative to screen, already done
                    x_offset: x_pos,
                    y_offset: y_pos,
                    static_x_offset: x_pos,
                    static_y_offset: y_pos,
                },
            };

            let parent_size = LayoutSize::new(parent_width.total(), parent_height.total());
            let child_size = LayoutSize::new(width.total(), height.total());
            let child_rect = LayoutRect::new(LayoutPoint::new(x_pos, y_pos), child_size);

            children_sum_rects.push(child_rect);

            let child_padding = get_padding(&child_rect_layout, parent_width.total(), parent_height.total());
            let child_margin = get_margin(&child_rect_layout, parent_width.total(), parent_height.total());
            let child_border_widths = get_border_widths(&child_rect_layout, parent_width.total(), parent_height.total());

            // set text, if any
            let child_text = if let (Some(words), Some((scaled_words, _)), Some((word_positions, _))) = (word_cache.get(&child_node_id), scaled_words.get(&child_node_id), word_positions_no_max_width.get(&child_node_id)) {
                let mut inline_text_layout = InlineText { words, scaled_words }.get_text_layout(*pipeline_id, child_node_id, &word_positions.text_layout_options);
                let (horz_alignment, vert_alignment) = determine_text_alignment(&display_rects[child_node_id]);
                inline_text_layout.align_children_horizontal(horz_alignment);
                inline_text_layout.align_children_vertical_in_parent_bounds(&parent_size, vert_alignment);
                let bounds = inline_text_layout.get_bounds();
                let glyphs = get_layouted_glyphs(word_positions, scaled_words, &inline_text_layout);
                glyph_map.insert(child_node_id, glyphs);
                Some((word_positions.text_layout_options.clone(), inline_text_layout, bounds))
            } else {
                None
            };

            let child_overflow = get_overflow(&solved_widths.layout_only_arena[child_node_id], &child_rect, &None);

            positioned_rects[child_node_id] = PositionedRectangle {
                size: child_size,
                position: child_position,
                padding: child_padding,
                margin: child_margin,
                border_widths: child_border_widths,
                resolved_text_layout_options: child_text,
                overflow: child_overflow,
            };
        }

        let children_sum_rect = LayoutRect::union(children_sum_rects.into_iter());
        let parent_overflow = get_overflow(parent_rect_layout, &parent_sum_rect, &children_sum_rect);

        positioned_rects[*parent_node_id] = PositionedRectangle {
            size: parent_size,
            position: parent_position_info,
            padding: parent_padding,
            margin: parent_margin,
            border_widths: parent_border_widths,
            resolved_text_layout_options: parent_text,
            overflow: parent_overflow
        };

        if parent_position != LayoutPosition::Static {
            positioned_node_stack.pop();
        }
    }

    LayoutResult {
        rects: positioned_rects,
        word_cache,
        scaled_words,
        positioned_word_cache: word_positions_no_max_width,
        layouted_glyph_cache: glyph_map,
        node_depths: solved_widths.non_leaf_nodes_sorted_by_depth,
    }
}

fn create_word_cache(
    app_resources: &AppResources,
    node_data: &NodeDataContainer<NodeData>,
) -> BTreeMap<NodeId, Words>
{
    use azul_text_layout::text_layout::split_text_into_words;
    node_data
    .linear_iter()
    .filter_map(|node_id| {
        match &node_data[node_id].get_node_type() {
            NodeType::Label(string) => Some((node_id, split_text_into_words(string.as_str()))),
            NodeType::Text(text_id) => {
                app_resources.get_text(text_id).map(|words| (node_id, words.clone()))
            },
            _ => None,
        }
    }).collect()
}

pub fn create_scaled_words(
    pipeline_id: &PipelineId,
    app_resources: &AppResources,
    words: &BTreeMap<NodeId, Words>,
    display_rects: &NodeDataContainer<DisplayRectangle>,
) -> BTreeMap<NodeId, (ScaledWords, FontInstanceKey)> {

    use azul_core::app_resources::{ImmediateFontId, font_size_to_au, get_font_id, get_font_size};
    use azul_text_layout::text_layout::words_to_scaled_words;

    words.iter().filter_map(|(node_id, words)| {

        let style = &display_rects[*node_id].style;
        let font_size = get_font_size(&style);
        let font_size_au = font_size_to_au(font_size);
        let css_font_id = get_font_id(&style);
        let font_id = match app_resources.get_css_font_id(css_font_id) {
            Some(s) => ImmediateFontId::Resolved(*s),
            None => ImmediateFontId::Unresolved(css_font_id.to_string()),
        };

        let loaded_font = app_resources.get_loaded_font(pipeline_id, &font_id)?;
        let font_instance_key = loaded_font.font_instances.get(&font_size_au)?;

        let scaled_words = words_to_scaled_words(
            words,
            &loaded_font.font_bytes,
            loaded_font.font_index as u32,
            loaded_font.font_metrics,
            font_size.inner.to_pixels(DEFAULT_FONT_SIZE_PX as f32),
        );

        Some((*node_id, (scaled_words, *font_instance_key)))
    }).collect()
}

fn create_word_positions(
    pipeline_id: &PipelineId,
    words: &BTreeMap<NodeId, Words>,
    scaled_words: &BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
    display_rects: &NodeDataContainer<DisplayRectangle>,
    solved_widths: &SolvedWidthLayout,
) -> BTreeMap<NodeId, (WordPositions, FontInstanceKey)> {

    use azul_text_layout::text_layout::position_words;
    use azul_core::ui_solver::{ResolvedTextLayoutOptions, DEFAULT_LETTER_SPACING, DEFAULT_WORD_SPACING};
    use azul_css::Overflow;
    use azul_core::app_resources::get_font_size;

    let mut word_positions = BTreeMap::new();

    for (node_id, words) in words.iter() {
        let rect = &display_rects[*node_id];
        let (scaled_words, font_instance_key) = match scaled_words.get(&node_id) {
            Some(s) => s,
            None => continue,
        };
        let font_size_px = get_font_size(&rect.style).inner.to_pixels(DEFAULT_FONT_SIZE_PX as f32);
        let text_can_overflow = rect.layout.overflow_x.unwrap_or_default().get_property_or_default().unwrap_or_default() != Overflow::Auto;
        let letter_spacing = rect.style.letter_spacing.and_then(|ls| Some(ls.get_property()?.inner.to_pixels(DEFAULT_LETTER_SPACING)));
        let word_spacing = rect.style.word_spacing.and_then(|ws| Some(ws.get_property()?.inner.to_pixels(DEFAULT_WORD_SPACING)));
        let line_height = rect.style.line_height.and_then(|lh| Some(lh.get_property()?.inner.get()));
        let tab_width = rect.style.tab_width.and_then(|tw| Some(tw.get_property()?.inner.get()));

        let text_layout_options = ResolvedTextLayoutOptions {
            max_horizontal_width: if text_can_overflow { Some(solved_widths.solved_widths[*node_id].total()) } else { None },
            leading: None, // TODO
            holes: Vec::new(), // TODO
            font_size_px,
            word_spacing,
            letter_spacing,
            line_height,
            tab_width,
        };

        word_positions.insert(*node_id, (position_words(words, scaled_words, &text_layout_options), *font_instance_key));
    }

    word_positions
}

/*
fn get_glyphs(
    node_hierarchy: &NodeHierarchy,
    scaled_words: &BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
    positioned_word_cache: &mut BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    display_rects: &NodeDataContainer<DisplayRectangle>,
    solved_widths: &SolvedWidthLayout,
    solved_heights: &SolvedHeightLayout,
) -> BTreeMap<NodeId, LayoutedGlyphs> {

    use azul_text_layout::text_layout::get_layouted_glyphs;

    scaled_words
    .iter()
    .filter_map(|(node_id, (scaled_words, _))| {


    }).collect()
}
*/

/// For a given rectangle, determines what text alignment should be used
fn determine_text_alignment(rect: &DisplayRectangle)
    -> (StyleTextAlignmentHorz, StyleTextAlignmentVert)
{
    let mut horz_alignment = StyleTextAlignmentHorz::default();
    let mut vert_alignment = StyleTextAlignmentVert::default();

    if let Some(align_items) = rect.layout.align_items {
        // Vertical text alignment
        use azul_css::LayoutAlignItems;
        match align_items.get_property_or_default().unwrap_or_default() {
            LayoutAlignItems::Start => vert_alignment = StyleTextAlignmentVert::Top,
            LayoutAlignItems::End => vert_alignment = StyleTextAlignmentVert::Bottom,
            // technically stretch = blocktext, but we don't have that yet
            _ => vert_alignment = StyleTextAlignmentVert::Center,
        }
    }

    if let Some(justify_content) = rect.layout.justify_content {
        use azul_css::LayoutJustifyContent;
        // Horizontal text alignment
        match justify_content.get_property_or_default().unwrap_or_default() {
            LayoutJustifyContent::Start => horz_alignment = StyleTextAlignmentHorz::Left,
            LayoutJustifyContent::End => horz_alignment = StyleTextAlignmentHorz::Right,
            _ => horz_alignment = StyleTextAlignmentHorz::Center,
        }
    }

    if let Some(text_align) = rect.style.text_align.and_then(|ta| ta.get_property().copied()) {
        // Horizontal text alignment with higher priority
        horz_alignment = text_align;
    }

    (horz_alignment, vert_alignment)
}
