use std::{f32, collections::BTreeMap};
use azul_css::{
    LayoutPosition, LayoutPoint, LayoutSize, LayoutRect,
    LayoutMarginTop, LayoutMarginRight, LayoutMarginBottom, LayoutMarginLeft,
    LayoutPaddingTop, LayoutPaddingRight, LayoutPaddingBottom, LayoutPaddingLeft,
    RectLayout, StyleTextAlignmentHorz, StyleTextAlignmentVert, CssPropertyValue,
};
use azul_core::{
    traits::GetTextLayout,
    id_tree::{NodeId, NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut},
    dom::{NodeData, NodeType},
    styled_dom::{StyledDom, DomId, StyledNode, AzNode, ParentWithNodeDepth},
    ui_solver::{DEFAULT_FONT_SIZE_PX, ScrolledNodes, ResolvedOffsets, LayoutResult, PositionedRectangle, OverflowInfo},
    app_resources::{AppResources, FontInstanceKey, FontImageApi},
    callbacks::PipelineId,
    display_list::RenderCallbacks,
    window::FullWindowState,
};
use azul_text_layout::{InlineText, text_layout::{Words, ShapedWords, WordPositions}};

const DEFAULT_FLEX_GROW_FACTOR: f32 = 1.0;

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum WhConstraint {
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
    pub(crate) fn $fn_name(styled_node: &StyledNode, preferred_inner_width: Option<f32>, parent_width: f32) -> WhConstraint {

        let mut width = styled_node.layout.$width.and_then(|w| w.get_property().map(|x| x.inner.to_pixels(parent_width)));
        let min_width = styled_node.layout.$min_width.and_then(|w| w.get_property().map(|x| x.inner.to_pixels(parent_width)));
        let max_width = styled_node.layout.$max_width.and_then(|w| w.get_property().map(|x| x.inner.to_pixels(parent_width)));

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
pub(crate) struct WidthCalculatedRect {
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
pub(crate) struct HeightCalculatedRect {
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
    /// We could operate on the NodeDataContainer<StyledNode> directly, but that makes testing very
    /// hard since we are only interested in testing or touching the layout. So this makes the
    /// calculation maybe a few microseconds slower, but gives better testing capabilities
    ///
    /// NOTE: Later on, this could maybe be a NodeDataContainer<&'a RectLayout>.
    #[must_use]
    pub(crate) fn $from_rect_layout_arena_fn_name<'a>(
        node_data: &NodeDataContainerRef<'a, StyledNode>,
        widths: &NodeDataContainerRef<'a, Option<f32>>,
        node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
        node_depths: &[ParentWithNodeDepth],
    ) -> NodeDataContainer<$struct_name> {

        // then calculate the widths again, but this time using the parent nodes
        let mut new_nodes = NodeDataContainer {
            internal: vec![$struct_name::default();node_data.len()]
        };

        for ParentWithNodeDepth { depth, node_id } in node_depths.iter() {

            let parent_id = match node_id.into_crate_internal() { Some(s) => s, None => continue, };

            let nd = &node_data[parent_id];
            let width = match widths.get(parent_id) { Some(s) => *s, None => continue, };
            let parent_width = node_hierarchy
            .get(parent_id)
            .and_then(|t| new_nodes.as_ref().get(t.parent_id()?).map(|parent| parent.$preferred_field))
            .unwrap_or_default()
            .min_needed_space()
            .unwrap_or(0.0);
            let parent_width = $determine_preferred_fn(&nd, width, parent_width);

            new_nodes.as_ref_mut()[parent_id] = $struct_name {
                // TODO: get the initial width of the rect content
                $preferred_field: parent_width,

                margin_top: nd.layout.margin_top.unwrap_or_default(),
                margin_left: nd.layout.margin_left.unwrap_or_default(),
                margin_right: nd.layout.margin_right.unwrap_or_default(),
                margin_bottom: nd.layout.margin_bottom.unwrap_or_default(),

                padding_top: nd.layout.padding_top.unwrap_or_default(),
                padding_left: nd.layout.padding_left.unwrap_or_default(),
                padding_right: nd.layout.padding_right.unwrap_or_default(),
                padding_bottom: nd.layout.padding_bottom.unwrap_or_default(),

                flex_grow_px: 0.0,
                min_inner_size_px: 0.0,
            };

            for child_id in parent_id.az_children(node_hierarchy) {
                let nd = &node_data[child_id];
                let width = match widths.get(child_id) { Some(s) => *s, None => continue, };
                new_nodes.as_ref_mut()[child_id] = $struct_name {
                    // TODO: get the initial width of the rect content
                    $preferred_field: $determine_preferred_fn(&nd, width, parent_width.min_needed_space().unwrap_or(0.0)),

                    margin_top: nd.layout.margin_top.unwrap_or_default(),
                    margin_left: nd.layout.margin_left.unwrap_or_default(),
                    margin_right: nd.layout.margin_right.unwrap_or_default(),
                    margin_bottom: nd.layout.margin_bottom.unwrap_or_default(),

                    padding_top: nd.layout.padding_top.unwrap_or_default(),
                    padding_left: nd.layout.padding_left.unwrap_or_default(),
                    padding_right: nd.layout.padding_right.unwrap_or_default(),
                    padding_bottom: nd.layout.padding_bottom.unwrap_or_default(),

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
    pub(crate) fn $bubble_fn_name<'a>(
        node_data: &'a mut NodeDataContainer<$struct_name>,
        node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
        arena_data: &NodeDataContainerRef<'a, StyledNode>,
        node_depths: &[ParentWithNodeDepth],
    ) {
        // Reverse, since we want to go from the inside out (depth 5 needs to be filled out first)
        //
        // Set the preferred_width of the parent nodes
        for ParentWithNodeDepth { depth, node_id } in node_depths.iter().rev() {

            use self::WhConstraint::*;

            let non_leaf_id = match node_id.into_crate_internal() { Some(s) => s, None => continue, };

            let parent_width = node_hierarchy[non_leaf_id].parent_id().and_then(|parent_id| node_data.as_ref()[parent_id].$preferred_field.min_needed_space()).unwrap_or(0.0);

            // Sum of the direct children's flex-basis = the parents preferred width
            let children_flex_basis = $sum_children_flex_basis_fn_name(&node_data.as_ref(), non_leaf_id, node_hierarchy, arena_data);

            // Calculate the new flex-basis width
            let parent_width_metrics = node_data.as_ref()[non_leaf_id];

            // For calculating the inner width, subtract the parents padding
            let parent_padding = node_data.as_ref()[non_leaf_id].$get_padding_fn(parent_width);

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

            node_data.as_ref_mut()[non_leaf_id].min_inner_size_px = child_width;
        }

        // Now, the width of all elements should be filled,
        // but they aren't flex-growed or flex-shrinked yet
    }

    /// Go from the root down and flex_grow the children if needed - respects the `width`, `min_width` and `max_width` properties
    /// The layout step doesn't account for the min_width and max_width constraints, so we have to adjust them manually
    pub(crate) fn $apply_flex_grow_fn_name<'a>(
        node_data: &'a mut NodeDataContainer<$struct_name>,
        node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
        arena_data: &NodeDataContainerRef<'a, StyledNode>,
        node_depths: &[ParentWithNodeDepth],
        root_width: f32
    ) {
        /// Does the actual width layout, respects the `width`, `min_width` and `max_width`
        /// properties as well as the `flex_grow` factor. `flex_shrink` currently does nothing.
        fn distribute_space_along_main_axis<'a>(
            node_id: &NodeId,
            node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
            arena_data: &NodeDataContainerRef<'a, StyledNode>,
            width_calculated_arena: &'a mut NodeDataContainer<$struct_name>,
            positioned_node_stack: &[NodeId],
            root_width: f32
        ) {
            // The inner space of the parent node, without the padding
            let mut parent_node_inner_width = {
                let parent_node = &width_calculated_arena.as_ref()[*node_id];
                let parent_parent_width = node_hierarchy[*node_id].parent_id().and_then(|p| width_calculated_arena.as_ref()[p].$preferred_field.max_available_space()).unwrap_or(root_width);
                parent_node.min_inner_size_px + parent_node.flex_grow_px - parent_node.$get_padding_fn(parent_parent_width)
            };

            println!("parent inner node width: {:?}", parent_node_inner_width);

            // 1. Set all child elements that have an exact width to that width, record their violations
            //    and add their violation to the leftover horizontal space.
            // let mut horizontal_space_from_fixed_width_items = 0.0;
            let mut horizontal_space_taken_up_by_fixed_width_items = 0.0;

            {
                // Vec<(NodeId, PreferredWidth)>
                let exact_width_childs = node_id
                        .az_children(node_hierarchy)
                        .filter_map(|id| if let WhConstraint::EqualTo(exact) = width_calculated_arena.as_ref()[id].$preferred_field {
                            Some((id, exact))
                        } else {
                            None
                        })
                        .collect::<Vec<(NodeId, f32)>>();

                for (exact_width_child_id, exact_width) in exact_width_childs {

                    // If this child node is `position: absolute`, it doesn't take any space away from
                    // its siblings, since it is taken out of the regular content flow
                    if arena_data[exact_width_child_id].layout.position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Absolute {
                        horizontal_space_taken_up_by_fixed_width_items += exact_width;
                    }

                    // so that node.min_inner_size_px + node.flex_grow_px = exact_width
                    let flex_grow_px = exact_width - width_calculated_arena.as_ref()[exact_width_child_id].min_inner_size_px;
                    width_calculated_arena.as_ref_mut()[exact_width_child_id].flex_grow_px = flex_grow_px;
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
                .az_children(node_hierarchy)
                .filter(|id| !width_calculated_arena.as_ref()[*id].$preferred_field.is_fixed_constraint())
                .collect::<FastHashSet<NodeId>>();

            let mut absolute_variable_width_nodes = Vec::new();

            for variable_child_id in &variable_width_childs {

                if arena_data[*variable_child_id].layout.position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Absolute {

                    let min_width = width_calculated_arena.as_ref()[*variable_child_id].$preferred_field.min_needed_space().unwrap_or(0.0);

                    horizontal_space_taken_up_by_variable_items += min_width;

                    // so that node.min_inner_size_px + node.flex_grow_px = min_width
                    let flex_grow_px = min_width - width_calculated_arena.as_ref()[*variable_child_id].min_inner_size_px;
                    width_calculated_arena.as_ref_mut()[*variable_child_id].flex_grow_px = flex_grow_px;

                } else {

                    // `position: absolute` items don't take space away from their siblings, rather
                    // they take the minimum needed space by their content

                    let root_id = NodeId::new(0);
                    let nearest_relative_parent_node = positioned_node_stack.get(positioned_node_stack.len() - 1).unwrap_or(&root_id);
                    let relative_parent_width = {
                        let relative_parent_node = &width_calculated_arena.as_ref()[*nearest_relative_parent_node];
                        relative_parent_node.flex_grow_px + relative_parent_node.min_inner_size_px
                    };

                    // By default, absolute positioned elements take the width of their content
                    // let min_inner_width = width_calculated_arena[*variable_child_id].$preferred_field.min_needed_space().unwrap_or(0.0);

                    // The absolute positioned node might have a max-width constraint, which has a
                    // higher precedence than `top, bottom, left, right`.
                    let max_space_current_node = match width_calculated_arena.as_ref()[*variable_child_id].$preferred_field {
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
                    let flex_grow_px = max_space_current_node - width_calculated_arena.as_ref()[*variable_child_id].min_inner_size_px;
                    width_calculated_arena.as_ref_mut()[*variable_child_id].flex_grow_px = flex_grow_px;

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
                            arena_data[*child_id].layout.flex_grow
                                .and_then(|g| g.get_property().copied())
                                .and_then(|grow| Some(grow.inner.get().max(1.0)))
                                .unwrap_or(DEFAULT_FLEX_GROW_FACTOR))
                    .sum();

                // Grow all variable children by the same amount.
                for variable_child_id in &variable_width_childs {

                    let flex_grow = arena_data[*variable_child_id].layout.flex_grow
                        .and_then(|g| g.get_property().copied())
                        .and_then(|grow| Some(grow.inner.get().max(1.0)))
                        .unwrap_or(DEFAULT_FLEX_GROW_FACTOR);

                    let added_space_for_one_child = total_horizontal_space_available * (flex_grow / children_combined_flex_grow);

                    let current_width_of_child = width_calculated_arena.as_ref()[*variable_child_id].min_inner_size_px +
                                                 width_calculated_arena.as_ref()[*variable_child_id].flex_grow_px;

                    if let Some(max_width) = width_calculated_arena.as_ref()[*variable_child_id].$preferred_field.max_available_space() {
                        if (current_width_of_child + added_space_for_one_child) > max_width {
                            // so that node.min_inner_size_px + node.flex_grow_px = max_width
                            let flex_grow_px = max_width - width_calculated_arena.as_ref()[*variable_child_id].min_inner_size_px;
                            width_calculated_arena.as_ref_mut()[*variable_child_id].flex_grow_px = flex_grow_px;

                            max_width_violations.push(*variable_child_id);
                        } else {
                            // so that node.min_inner_size_px + node.flex_grow_px = added_space_for_one_child
                            let flex_grow_px = added_space_for_one_child - width_calculated_arena.as_ref()[*variable_child_id].min_inner_size_px;
                            width_calculated_arena.as_ref_mut()[*variable_child_id].flex_grow_px = flex_grow_px;
                        }
                    } else {
                        // so that node.min_inner_size_px + node.flex_grow_px = added_space_for_one_child
                        let flex_grow_px = added_space_for_one_child - width_calculated_arena.as_ref()[*variable_child_id].min_inner_size_px;
                        width_calculated_arena.as_ref_mut()[*variable_child_id].flex_grow_px = flex_grow_px;
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
                            width_calculated_arena.as_ref()[solved_node_id].min_inner_size_px +
                            width_calculated_arena.as_ref()[solved_node_id].flex_grow_px;

                        variable_width_childs.remove(&solved_node_id);
                    }
                }
            }
        }

        fn distribute_space_along_cross_axis<'a>(
            node_id: &NodeId,
            node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
            arena_data: &NodeDataContainerRef<'a, StyledNode>,
            width_calculated_arena: &'a mut NodeDataContainer< $struct_name>,
            positioned_node_stack: &[NodeId],
            root_width: f32
        ) {
            // The inner space of the parent node, without the padding
            let parent_node_inner_width = {
                let parent_node = &width_calculated_arena.as_ref()[*node_id];
                let parent_parent_width = node_hierarchy[*node_id].parent_id().and_then(|p| width_calculated_arena.as_ref()[p].$preferred_field.max_available_space()).unwrap_or(root_width);
                parent_node.min_inner_size_px + parent_node.flex_grow_px - parent_node.$get_padding_fn(parent_parent_width)
            };

            let last_relative_node_width = {
                let zero_node = NodeId::new(0);
                let last_relative_node_id = positioned_node_stack.get(positioned_node_stack.len() - 1).unwrap_or(&zero_node);
                let last_relative_node = &width_calculated_arena.as_ref()[*last_relative_node_id];
                let last_relative_node_parent_width = node_hierarchy[*last_relative_node_id].parent_id().and_then(|p| width_calculated_arena.as_ref()[p].$preferred_field.max_available_space()).unwrap_or(root_width);
                last_relative_node.min_inner_size_px + last_relative_node.flex_grow_px - last_relative_node.$get_padding_fn(last_relative_node_parent_width)
            };

            for child_id in node_id.az_children(node_hierarchy) {

                let parent_node_inner_width = if arena_data[child_id].layout.position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Absolute {
                    parent_node_inner_width
                } else {
                    last_relative_node_width
                };

                let preferred_width = {
                    let min_width = width_calculated_arena.as_ref()[child_id].$preferred_field.min_needed_space().unwrap_or(0.0);
                    // In this case we want to overflow if the min width of the cross axis
                    if min_width > parent_node_inner_width {
                        min_width
                    } else {
                        if let Some(max_width) = width_calculated_arena.as_ref()[child_id].$preferred_field.max_available_space() {
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
                let flex_grow_px = preferred_width - width_calculated_arena.as_ref()[child_id].min_inner_size_px;
                width_calculated_arena.as_ref_mut()[child_id].flex_grow_px = flex_grow_px;
            }
        }

        debug_assert!(node_data.as_ref()[NodeId::ZERO].flex_grow_px == 0.0);

        // Set the window width on the root node (since there is only one root node, we can
        // calculate the `flex_grow_px` directly)
        //
        // Usually `top_level_flex_basis` is NOT 0.0, rather it's the sum of all widths in the DOM,
        // i.e. the sum of the whole DOM tree
        let top_level_flex_basis = node_data.as_ref()[NodeId::ZERO].min_inner_size_px;

        // The root node can still have some sort of max-width attached, so we need to check for that
        let root_preferred_width = if let Some(max_width) = node_data.as_ref()[NodeId::ZERO].$preferred_field.max_available_space() {
            if root_width > max_width { max_width } else { root_width }
        } else {
            root_width
        };

        node_data.as_ref_mut()[NodeId::ZERO].flex_grow_px = root_preferred_width - top_level_flex_basis;

        // Keep track of the nearest relative or absolute positioned element
        let mut positioned_node_stack = vec![NodeId::new(0)];

        for ParentWithNodeDepth { depth, node_id } in node_depths.iter() {

            let parent_id = match node_id.into_crate_internal() { Some(s) => s, None => continue, };

            use azul_css::{LayoutAxis, LayoutPosition};

            let parent_is_positioned = arena_data[parent_id].layout.position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Static;
            if parent_is_positioned {
                positioned_node_stack.push(parent_id);
            }

            if arena_data[parent_id].layout.direction.unwrap_or_default().get_property_or_default().unwrap_or_default().get_axis() == LayoutAxis::$main_axis {
                distribute_space_along_main_axis(&parent_id, node_hierarchy, arena_data, node_data, &positioned_node_stack, root_width);
            } else {
                distribute_space_along_cross_axis(&parent_id, node_hierarchy, arena_data, node_data, &positioned_node_stack, root_width);
            }

            if parent_is_positioned {
                positioned_node_stack.pop();
            }
        }
    }

    /// Returns the sum of the flex-basis of the current nodes' children
    #[must_use]
    pub(crate) fn $sum_children_flex_basis_fn_name<'a>(
        node_data: &NodeDataContainerRef<'a, $struct_name>,
        node_id: NodeId,
        node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
        display_arena: &NodeDataContainerRef<'a, StyledNode>
    ) -> f32 {
        let parent_width = node_data[node_id].$preferred_field.max_available_space().unwrap_or(0.0);
        node_id
            .az_children(node_hierarchy)
            .filter(|child_node_id| display_arena[*child_node_id].layout.position.and_then(|p| p.get_property().copied()) != Some(LayoutPosition::Absolute))
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
}

#[derive(Debug, Clone)]
pub(crate) struct SolvedHeightLayout {
    pub solved_heights: NodeDataContainer<HeightSolvedResult>,
}

/// Returns the solved widths of the items in a BTree form
pub(crate) fn solve_flex_layout_width<'a>(
    styled_dom: &StyledDom,
    preferred_widths: &NodeDataContainerRef<'a, Option<f32>>,
    window_width: f32
) -> SolvedWidthLayout {
    let mut width_calculated_arena = width_calculated_rect_arena_from_rect_layout_arena(&styled_dom.styled_nodes.as_container(), preferred_widths, &styled_dom.node_hierarchy.as_container(), &styled_dom.non_leaf_nodes.as_ref());
    bubble_preferred_widths_to_parents(&mut width_calculated_arena, &styled_dom.node_hierarchy.as_container(), &styled_dom.styled_nodes.as_container(), &styled_dom.non_leaf_nodes.as_ref());
    width_calculated_rect_arena_apply_flex_grow(&mut width_calculated_arena, &styled_dom.node_hierarchy.as_container(), &styled_dom.styled_nodes.as_container(), &styled_dom.non_leaf_nodes.as_ref(), window_width);
    let solved_widths = width_calculated_arena.as_ref().transform(|node, _| node.solved_result());
    SolvedWidthLayout { solved_widths }
}

/// Returns the solved height of the items in a BTree form
pub(crate) fn solve_flex_layout_height<'a>(
    styled_dom: &StyledDom,
    preferred_heights: &NodeDataContainerRef<'a, Option<f32>>,
    window_height: f32
) -> SolvedHeightLayout {
    let mut height_calculated_arena = height_calculated_rect_arena_from_rect_layout_arena(&styled_dom.styled_nodes.as_container(), preferred_heights, &styled_dom.node_hierarchy.as_container(), &styled_dom.non_leaf_nodes.as_ref());
    bubble_preferred_heights_to_parents(&mut height_calculated_arena, &styled_dom.node_hierarchy.as_container(), &styled_dom.styled_nodes.as_container(), &styled_dom.non_leaf_nodes.as_ref());
    height_calculated_rect_arena_apply_flex_grow(&mut height_calculated_arena, &styled_dom.node_hierarchy.as_container(), &styled_dom.styled_nodes.as_container(), &styled_dom.non_leaf_nodes.as_ref(), window_height);
    let solved_heights = height_calculated_arena.as_ref().transform(|node, _| node.solved_result());
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
fn $fn_name<'a>(
    node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
    node_data: &NodeDataContainerRef<'a, StyledNode>,
    node_depths: &[ParentWithNodeDepth],
    solved_widths: &$width_layout)
-> NodeDataContainer<$height_solved_position>
{
    fn determine_child_x_absolute<'a>(
        child_id: NodeId,
        positioned_node_stack: &[NodeId],
        arena_data: &NodeDataContainerRef<'a, StyledNode>,
        arena_solved_data: &mut NodeDataContainerRefMut<'a, $height_solved_position>,
        solved_widths: &$width_layout,
        node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
    ) {
        let child_width_with_padding = {
            let child_node = &solved_widths.$solved_widths_field.as_ref()[child_id];
            child_node.$min_width + child_node.space_added
        };

        let child_node = &arena_data[child_id];
        let child_node_parent_width = node_hierarchy[child_id].parent_id().map(|p| solved_widths.$solved_widths_field.as_ref()[p].total()).unwrap_or(0.0);
        let child_margin_left = child_node.layout.$margin_left.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);
        let child_margin_right = child_node.layout.$margin_right.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);

        let zero_node = NodeId::new(0);
        let last_relative_node_id = positioned_node_stack.get(positioned_node_stack.len() - 1).unwrap_or(&zero_node);

        let last_relative_node = &arena_data[*last_relative_node_id];
        let last_relative_padding_left = last_relative_node.layout.$padding_left.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);
        let last_relative_padding_right = last_relative_node.layout.$padding_right.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);

        let last_relative_node_x = arena_solved_data[*last_relative_node_id].0 + last_relative_padding_left;
        let last_relative_node_inner_width = {
            let last_relative_node = &solved_widths.$solved_widths_field.as_ref()[*last_relative_node_id];
            last_relative_node.$min_width + last_relative_node.space_added - (last_relative_padding_left + last_relative_padding_right)
        };

        let child_left = &arena_data[child_id].layout.$left.and_then(|s| Some(s.get_property()?.inner.to_pixels(child_node_parent_width)));
        let child_right = &arena_data[child_id].layout.$right.and_then(|s| Some(s.get_property()?.inner.to_pixels(child_node_parent_width)));

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

    fn determine_child_x_along_main_axis<'a>(
        main_axis_alignment: LayoutJustifyContent,
        arena_data: &NodeDataContainerRef<'a, StyledNode>,
        arena_solved_data: &mut NodeDataContainerRefMut<'a, $height_solved_position>,
        solved_widths: &$width_layout,
        child_id: NodeId,
        parent_x_position: f32,
        parent_inner_width: f32,
        sum_x_of_children_so_far: &mut f32,
        positioned_node_stack: &[NodeId],
        node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
    ) {
        use azul_css::LayoutJustifyContent::*;

        let child_width_with_padding = {
            let child_node = &solved_widths.$solved_widths_field.as_ref()[child_id];
            child_node.$min_width + child_node.space_added
        };

        // width: increase X according to the main axis, Y according to the cross_axis
        let child_node = &arena_data[child_id];
        let child_node_parent_width = node_hierarchy[child_id].parent_id().map(|p| solved_widths.$solved_widths_field.as_ref()[p].total()).unwrap_or(0.0);
        let child_margin_left = child_node.layout.$margin_left.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);
        let child_margin_right = child_node.layout.$margin_right.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);

        if child_node.layout.position.unwrap_or_default().get_property_or_default().unwrap_or_default() == LayoutPosition::Absolute {
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

    fn determine_child_x_along_cross_axis<'a>(
        arena_data: &NodeDataContainerRef<'a, StyledNode>,
        solved_widths: &$width_layout,
        child_id: NodeId,
        positioned_node_stack: &[NodeId],
        arena_solved_data: &mut NodeDataContainerRefMut<'a, $height_solved_position>,
        parent_x_position: f32,
        node_hierarchy: &NodeDataContainerRef<'a, AzNode>
    ) {
        let child_node = &arena_data[child_id];
        let child_node_parent_width = node_hierarchy[child_id].parent_id().map(|p| solved_widths.$solved_widths_field.as_ref()[p].total()).unwrap_or(0.0);
        let child_margin_left = child_node.layout.$margin_left.and_then(|x| Some(x.get_property()?.inner.to_pixels(child_node_parent_width))).unwrap_or(0.0);

        if child_node.layout.position.unwrap_or_default().get_property_or_default().unwrap_or_default() == LayoutPosition::Absolute {
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

    for ParentWithNodeDepth { depth, node_id } in node_depths.iter() {

        let parent_id = match node_id.into_crate_internal() { Some(s) => s, None => continue, };
        let parent_node = &node_data[parent_id];
        let parent_parent_width = node_hierarchy[parent_id].parent_id().map(|p| solved_widths.$solved_widths_field.as_ref()[p].total()).unwrap_or(0.0);
        let parent_padding_left = parent_node.layout.$padding_left.and_then(|x| Some(x.get_property()?.inner.to_pixels(parent_parent_width))).unwrap_or(0.0);
        let parent_padding_right = parent_node.layout.$padding_right.and_then(|x| Some(x.get_property()?.inner.to_pixels(parent_parent_width))).unwrap_or(0.0);
        let parent_x_position = arena_solved_data.as_ref()[parent_id].0 + parent_padding_left;
        let parent_direction = parent_node.layout.direction.and_then(|g| g.get_property_or_default()).unwrap_or_default();

        // Push nearest relative or absolute positioned element
        let parent_is_positioned = parent_node.layout.position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Static;
        if parent_is_positioned {
            positioned_node_stack.push(parent_id);
        }

        let parent_inner_width = {
            let parent_node = &solved_widths.$solved_widths_field.as_ref()[parent_id];
            parent_node.$min_width + parent_node.space_added - (parent_padding_left + parent_padding_right)
        };

        if parent_direction.get_axis() == LayoutAxis::$axis {
            // Along main axis: Take X of parent
            let main_axis_alignment = node_data[parent_id].layout.justify_content.unwrap_or_default().get_property_or_default().unwrap_or_default();
            let mut sum_x_of_children_so_far = 0.0;

            if parent_direction.is_reverse() {
                for child_id in parent_id.az_reverse_children(node_hierarchy) {
                    determine_child_x_along_main_axis(
                        main_axis_alignment,
                        node_data,
                        &mut arena_solved_data.as_ref_mut(),
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
                for child_id in parent_id.az_children(node_hierarchy) {
                    determine_child_x_along_main_axis(
                        main_axis_alignment,
                        node_data,
                        &mut arena_solved_data.as_ref_mut(),
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
                for child_id in parent_id.az_children(node_hierarchy).filter(|ch| {
                    node_data[*ch].layout.position.unwrap_or_default().get_property_or_default().unwrap_or_default() != LayoutPosition::Absolute
                }) {
                    arena_solved_data.as_ref_mut()[child_id].0 += diff;
                }
            }

        } else {
            // Along cross axis: Increase X with width of current element

            if parent_direction.is_reverse() {
                for child_id in parent_id.az_reverse_children(node_hierarchy) {
                    determine_child_x_along_cross_axis(
                        node_data,
                        solved_widths,
                        child_id,
                        &positioned_node_stack,
                        &mut arena_solved_data.as_ref_mut(),
                        parent_x_position,
                        node_hierarchy,
                    );
                }
            } else {
                for child_id in parent_id.az_children(node_hierarchy) {
                    determine_child_x_along_cross_axis(
                        node_data,
                        solved_widths,
                        child_id,
                        &positioned_node_stack,
                        &mut arena_solved_data.as_ref_mut(),
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

fn get_x_positions<'a>(
    solved_widths: &SolvedWidthLayout,
    node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
    node_data: &NodeDataContainerRef<'a, StyledNode>,
    node_depths: &[ParentWithNodeDepth],
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
    let mut arena = get_pos_x(node_hierarchy, node_data, node_depths, solved_widths);

    // Add the origin on top of the position
    let x = origin.x as f32;
    if x > 0.5 || x < -0.5 {
        for item in &mut arena.internal {
            item.0 += x;
        }
    }
    arena
}

fn get_y_positions<'a>(
    solved_heights: &SolvedHeightLayout,
    solved_widths: &SolvedWidthLayout,
    node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
    node_data: &NodeDataContainerRef<'a, StyledNode>,
    node_depths: &[ParentWithNodeDepth],
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
    let mut arena = get_pos_y(node_hierarchy, node_data, node_depths, solved_heights);

    // Add the origin on top of the position
    let y = origin.y as f32;
    if y > 0.5  || y < -0.5 {
        for item in &mut arena.internal {
            item.0 += y;
        }
    }
    arena
}

#[inline]
fn get_layout_positions<'a>(display_rects: &NodeDataContainerRef<'a, StyledNode>) -> NodeDataContainer<LayoutPosition> {
    display_rects.transform(|node, _| node.layout.position.unwrap_or_default().get_property_or_default().unwrap_or_default())
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

// Adds the image and font resources to the app_resources but does NOT add them to the RenderAPI
pub fn do_the_layout<U: FontImageApi>(
    styled_dom: StyledDom,
    app_resources: &mut AppResources,
    render_api: &mut U,
    pipeline_id: PipelineId,
    callbacks: RenderCallbacks<U>,
    full_window_state: &FullWindowState,
) -> Vec<LayoutResult> {

    use azul_core::callbacks::DomNodeId;

    let mut current_dom_id = 0;
    let mut doms = vec![
        (None, DomId { inner: current_dom_id }, styled_dom, LayoutRect::new(LayoutPoint::zero(), LayoutSize::new(full_window_state.size.dimensions.width, full_window_state.size.dimensions.height))),
    ];
    let mut resolved_doms = Vec::new();

    loop {

        let mut new_doms = Vec::new();

        for (parent_dom_id, dom_id, styled_dom, rect) in doms.drain(..) {

            use azul_core::app_resources::add_fonts_and_images;

            add_fonts_and_images(
                app_resources,
                render_api,
                &pipeline_id,
                &styled_dom.styled_nodes.as_ref(),
                &styled_dom.node_data.as_ref(),
                callbacks.load_font_fn,
                callbacks.load_image_fn,
                callbacks.parse_font_fn,
            );

            let mut layout_result = do_the_layout_internal(
                dom_id,
                parent_dom_id,
                styled_dom,
                app_resources,
                pipeline_id,
                rect,
            );

            let iframes = layout_result.styled_dom.scan_for_iframe_callbacks();

            let mut iframe_mapping = Vec::new();

            for (iframe_id, (node_id, iframe_node)) in iframes.iter().enumerate() {

                use azul_core::callbacks::{HidpiAdjustedBounds, IFrameCallbackInfoPtr, IFrameCallbackInfo};
                use azul_core::styled_dom::StyleOptions;
                use std::ffi::c_void;

                // Generate a new DomID
                current_dom_id += 1;
                let iframe_dom_id = DomId { inner: current_dom_id };
                iframe_mapping.push((*node_id, iframe_dom_id));

                let cb = iframe_node.callback;
                let ptr = &iframe_node.data;
                let bounds = &layout_result.rects.as_ref()[*node_id];
                let hidpi_bounds = HidpiAdjustedBounds::from_bounds(bounds.size, full_window_state.size.hidpi_factor);

                // Invoke the IFrame
                let mut iframe_dom: StyledDom = {

                    let callback_info = IFrameCallbackInfo {
                        state: ptr,
                        resources: &app_resources,
                        bounds: hidpi_bounds,
                    };

                    let ptr = IFrameCallbackInfoPtr { ptr: Box::into_raw(Box::new(callback_info)) as *mut c_void };
                    (cb.cb)(ptr).styled_dom
                };

                let hovered_nodes = full_window_state.hovered_nodes.get(&iframe_dom_id).cloned().unwrap_or_default().keys().cloned().collect::<Vec<_>>();
                let active_nodes = if !full_window_state.mouse_state.mouse_down() { Vec::new() } else { hovered_nodes.clone() };
                let mut focused_nodes = Vec::new();
                if let Some(DomNodeId { dom, node }) = full_window_state.get_previous_window_state().and_then(|b| b.focused_node) {
                    if dom == iframe_dom_id { focused_nodes.push(node.into_crate_internal().unwrap()); }
                }
                if let Some(DomNodeId { dom, node }) = full_window_state.focused_node {
                    if dom == iframe_dom_id { focused_nodes.push(node.into_crate_internal().unwrap()); }
                }

                if !hovered_nodes.is_empty() { iframe_dom.styled_nodes.restyle_nodes_hover(hovered_nodes.as_slice()); }
                if !active_nodes.is_empty() { iframe_dom.styled_nodes.restyle_nodes_hover(active_nodes.as_slice()); }
                if !focused_nodes.is_empty() { iframe_dom.styled_nodes.restyle_nodes_hover(focused_nodes.as_slice()); }

                let iframe_size = hidpi_bounds.get_logical_size();
                let bounds = LayoutRect::new(LayoutPoint::zero(), LayoutSize::new(iframe_size.width, iframe_size.height)); // TODO: use the iframe static position here?
                // push the styled iframe dom into the next iframes and repeat (recurse)
                new_doms.push((Some(dom_id), iframe_dom_id, iframe_dom, bounds));
            }

            layout_result.iframe_mapping = iframe_mapping;
            resolved_doms.push(layout_result);
        }

        if new_doms.is_empty() {
            break;
        } else {
            doms = new_doms;
        }
    }

    resolved_doms
}

/// At this point in time, all font keys, image keys, etc. have to be already
/// been submitted to the RenderApi and the AppResources!
pub fn do_the_layout_internal(
    dom_id: DomId,
    parent_dom_id: Option<DomId>,
    styled_dom: StyledDom,
    app_resources: &mut AppResources,
    pipeline_id: PipelineId,
    bounds: LayoutRect
) -> LayoutResult {

    use azul_core::ui_solver::PositionInfo;
    use azul_text_layout::text_layout::get_layouted_glyphs;

    let rect_size = bounds.size;
    let rect_offset = bounds.origin;

    // TODO: Filter all inline text blocks: inline blocks + their padding + margin
    // The NodeId has to be the **next** NodeId (the next sibling after the inline element)
    // let mut inline_text_blocks = BTreeMap::<NodeId, InlineText>::new();

    let content_widths_pre = NodeDataContainer { internal: vec![None; styled_dom.node_data.len()] };
    let solved_widths = solve_flex_layout_width(
        &styled_dom,
        &content_widths_pre.as_ref(),
        rect_size.width as f32,
    );

    // Break all strings into words and / or resolve the TextIds
    let word_cache = create_word_cache(app_resources, &styled_dom.node_data.as_container());
    // Scale the words to the correct size - TODO: Cache this in the app_resources!
    let shaped_words = create_shaped_words(&pipeline_id, app_resources, &word_cache, &styled_dom.styled_nodes.as_container());
    // Layout all words as if there was no max-width constraint (to get the texts "content width").
    let word_positions_no_max_width = create_word_positions(&pipeline_id, app_resources, &word_cache, &shaped_words, &styled_dom.styled_nodes.as_container(), &solved_widths);

    // Determine the preferred **content** width
    // let content_widths = node_data.transform(|node, node_id| {
    //     get_content_width(pipeline_id, &node_id, &node.get_node_type(), app_resources, &word_positions_no_max_width)
    // });

    // // Layout the words again, this time with the proper width constraints!
    // let proper_max_widths = solved_widths.solved_widths.linear_iter().map(|node_id| {
    //     (node_id, TextSizePx(solved_widths.solved_widths[node_id].total()))
    // }).collect();

    // let word_positions_with_max_width = create_word_positions(&word_cache, &shaped_words, display_rects, &proper_max_widths, &inline_text_blocks);

    // Get the content height of the content
    // let content_heights = node_data.transform(|node, node_id| {
    //     let div_width = solved_widths.solved_widths[node_id].total();
    //     get_content_height(pipeline_id, &node_id, &node.get_node_type(), app_resources, &word_positions_no_max_width, div_width)
    // });

    let content_heights_pre = NodeDataContainer { internal: vec![None; styled_dom.node_data.len()] };

    // TODO: The content height is not the final height!
    let solved_heights = solve_flex_layout_height(
        &styled_dom,
        &content_heights_pre.as_ref(),
        rect_size.height as f32,
    );

    let x_positions = get_x_positions(&solved_widths, &styled_dom.node_hierarchy.as_container(), &styled_dom.styled_nodes.as_container(), &styled_dom.non_leaf_nodes.as_ref(), rect_offset.clone());
    let y_positions = get_y_positions(&solved_heights, &solved_widths, &styled_dom.node_hierarchy.as_container(), &styled_dom.styled_nodes.as_container(), &styled_dom.non_leaf_nodes.as_ref(), rect_offset);
    let position_info = get_layout_positions(&styled_dom.styled_nodes.as_container());
    let mut glyph_map = BTreeMap::new();

    let mut positioned_rects = NodeDataContainer { internal: vec![PositionedRectangle::default(); styled_dom.node_data.len()].into() };
    let mut positioned_node_stack = vec![NodeId::new(0)];

    // create the final positioned rectangles
    for ParentWithNodeDepth { depth, node_id } in styled_dom.non_leaf_nodes.as_ref().iter() {

        let parent_node_id = match node_id.into_crate_internal() { Some(s) => s, None => continue, };

        let parent_rect_layout = &styled_dom.styled_nodes.as_container()[parent_node_id].layout;
        let parent_position = position_info.as_ref()[parent_node_id];
        let width = solved_widths.solved_widths.as_ref()[parent_node_id];
        let height = solved_heights.solved_heights.as_ref()[parent_node_id];
        let x_pos = x_positions.as_ref()[parent_node_id].0;
        let y_pos = y_positions.as_ref()[parent_node_id].0;

        let parent_parent_node_id = styled_dom.node_hierarchy.as_container()[parent_node_id].parent_id().unwrap_or(NodeId::new(0));
        let parent_x_pos = x_positions.as_ref()[parent_parent_node_id].0;
        let parent_y_pos = y_positions.as_ref()[parent_parent_node_id].0;
        let parent_parent_width = solved_widths.solved_widths.as_ref()[parent_parent_node_id];
        let parent_parent_height = solved_heights.solved_heights.as_ref()[parent_parent_node_id];

        let last_positioned_item_node_id = positioned_node_stack.last().map(|l| *l).unwrap_or(NodeId::new(0));
        let last_positioned_item_x_pos = x_positions.as_ref()[last_positioned_item_node_id].0;
        let last_positioned_item_y_pos = y_positions.as_ref()[last_positioned_item_node_id].0;
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
            positioned_node_stack.push(parent_node_id);
        }

        // set text, if any
        let parent_text = if let (Some(words), Some(shaped_words), Some((word_positions, _))) = (word_cache.get(&parent_node_id), shaped_words.get(&parent_node_id), word_positions_no_max_width.get(&parent_node_id)) {
            let mut inline_text_layout = InlineText { words, shaped_words }.get_text_layout(pipeline_id, parent_node_id, &word_positions.text_layout_options);
            let (horz_alignment, vert_alignment) = determine_text_alignment(&styled_dom.styled_nodes.as_container()[parent_node_id]);
            inline_text_layout.align_children_horizontal(horz_alignment);
            inline_text_layout.align_children_vertical_in_parent_bounds(&parent_parent_size, vert_alignment);
            let bounds = inline_text_layout.get_bounds();
            let glyphs = get_layouted_glyphs(word_positions, shaped_words, &inline_text_layout);
            glyph_map.insert(parent_node_id, glyphs);
            Some((word_positions.text_layout_options.clone(), inline_text_layout, bounds))
        } else {
            None
        };

        for child_node_id in parent_node_id.az_children(&styled_dom.node_hierarchy.as_container()) {

            // copy the width and height from the parent node
            let parent_width = width;
            let parent_height = height;
            let parent_x_pos = x_pos;
            let parent_y_pos = y_pos;

            let width = solved_widths.solved_widths.as_ref()[child_node_id];
            let height = solved_heights.solved_heights.as_ref()[child_node_id];
            let x_pos = x_positions.as_ref()[child_node_id].0;
            let y_pos = y_positions.as_ref()[child_node_id].0;
            let child_rect_layout = &styled_dom.styled_nodes.as_container()[child_node_id].layout;
            let child_position = position_info.as_ref()[child_node_id];

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
            let child_text = if let (Some(words), Some(shaped_words), Some((word_positions, _))) = (word_cache.get(&child_node_id), shaped_words.get(&child_node_id), word_positions_no_max_width.get(&child_node_id)) {
                let mut inline_text_layout = InlineText { words, shaped_words }.get_text_layout(pipeline_id, child_node_id, &word_positions.text_layout_options);
                let (horz_alignment, vert_alignment) = determine_text_alignment(&styled_dom.styled_nodes.as_container()[child_node_id]);
                inline_text_layout.align_children_horizontal(horz_alignment);
                inline_text_layout.align_children_vertical_in_parent_bounds(&parent_size, vert_alignment);
                let bounds = inline_text_layout.get_bounds();
                let glyphs = get_layouted_glyphs(word_positions, shaped_words, &inline_text_layout);
                glyph_map.insert(child_node_id, glyphs);
                Some((word_positions.text_layout_options.clone(), inline_text_layout, bounds))
            } else {
                None
            };

            let child_overflow = get_overflow(&child_rect_layout, &child_rect, &None);

            positioned_rects.as_ref_mut()[child_node_id] = PositionedRectangle {
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
        let parent_overflow = get_overflow(&parent_rect_layout, &parent_sum_rect, &children_sum_rect);

        positioned_rects.as_ref_mut()[parent_node_id] = PositionedRectangle {
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

    let overflowing_rects = get_nodes_that_need_scroll_clip(
        &styled_dom.styled_nodes.as_container(),
        &styled_dom.node_data.as_container(),
        &positioned_rects.as_ref(),
        styled_dom.non_leaf_nodes.as_ref(),
        pipeline_id,
    );

    LayoutResult {
        dom_id,
        parent_dom_id,
        styled_dom,
        rects: positioned_rects,
        word_cache,
        shaped_words,
        positioned_word_cache: word_positions_no_max_width,
        layouted_glyph_cache: glyph_map,
        scrollable_nodes: overflowing_rects,
        iframe_mapping: Vec::new(),
    }
}

fn create_word_cache<'a>(
    app_resources: &AppResources,
    node_data: &NodeDataContainerRef<'a, NodeData>,
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

pub fn create_shaped_words<'a>(
    pipeline_id: &PipelineId,
    app_resources: &mut AppResources,
    words: &BTreeMap<NodeId, Words>,
    display_rects: &'a NodeDataContainerRef<'a, StyledNode>,
) -> BTreeMap<NodeId, ShapedWords> {

    use azul_core::app_resources::{ImmediateFontId, get_font_id};
    use azul_text_layout::text_layout::shape_words;

    words.iter().filter_map(|(node_id, words)| {

        let style = &display_rects[*node_id].style;
        let css_font_id = get_font_id(&style);
        let font_id = match app_resources.get_css_font_id(css_font_id) {
            Some(s) => ImmediateFontId::Resolved(*s),
            None => ImmediateFontId::Unresolved(css_font_id.to_string()),
        };

        let loaded_font = app_resources.get_loaded_font_mut(pipeline_id, &font_id)?;

        // downcast the loaded_font.font from Box<dyn Any> to Box<azul_text_layout::text_shaping::ParsedFont>
        let parsed_font_downcasted = loaded_font.font.downcast_mut::<azul_text_layout::text_shaping::ParsedFont>()?;

        let shaped_words = shape_words(words, parsed_font_downcasted);

        Some((*node_id, shaped_words))
    }).collect()
}

fn create_word_positions<'a>(
    pipeline_id: &PipelineId,
    app_resources: &AppResources,
    words: &BTreeMap<NodeId, Words>,
    shaped_words: &BTreeMap<NodeId, ShapedWords>,
    display_rects: &'a NodeDataContainerRef<'a, StyledNode>,
    solved_widths: &SolvedWidthLayout,
) -> BTreeMap<NodeId, (WordPositions, FontInstanceKey)> {

    use azul_text_layout::text_layout::position_words;
    use azul_core::ui_solver::{ResolvedTextLayoutOptions, DEFAULT_LETTER_SPACING, DEFAULT_WORD_SPACING};
    use azul_css::Overflow;
    use azul_core::app_resources::{ImmediateFontId, font_size_to_au, get_font_id, get_font_size};

    words.iter().filter_map(|(node_id, words)| {
        let rect = &display_rects[*node_id];

        let font_size = get_font_size(&rect.style);
        let font_size_au = font_size_to_au(font_size);
        let font_size_px = font_size.inner.to_pixels(DEFAULT_FONT_SIZE_PX as f32);
        let css_font_id = get_font_id(&rect.style);

        let font_id = match app_resources.get_css_font_id(css_font_id) {
            Some(s) => ImmediateFontId::Resolved(*s),
            None => ImmediateFontId::Unresolved(css_font_id.to_string()),
        };
        let loaded_font = app_resources.get_loaded_font(pipeline_id, &font_id)?;
        let font_instance_key = loaded_font.font_instances.get(&font_size_au)?;

        let shaped_words = shaped_words.get(&node_id)?;
        let text_can_overflow = rect.layout.overflow_x.unwrap_or_default().get_property_or_default().unwrap_or_default() != Overflow::Auto;
        let letter_spacing = rect.style.letter_spacing.and_then(|ls| Some(ls.get_property()?.inner.to_pixels(DEFAULT_LETTER_SPACING)));
        let word_spacing = rect.style.word_spacing.and_then(|ws| Some(ws.get_property()?.inner.to_pixels(DEFAULT_WORD_SPACING)));
        let line_height = rect.style.line_height.and_then(|lh| Some(lh.get_property()?.inner.get()));
        let tab_width = rect.style.tab_width.and_then(|tw| Some(tw.get_property()?.inner.get()));

        let text_layout_options = ResolvedTextLayoutOptions {
            max_horizontal_width: if text_can_overflow { Some(solved_widths.solved_widths.as_ref()[*node_id].total()) } else { None },
            leading: None, // TODO
            holes: Vec::new(), // TODO
            font_size_px,
            word_spacing,
            letter_spacing,
            line_height,
            tab_width,
        };

        Some((*node_id, (position_words(words, shaped_words, &text_layout_options), *font_instance_key)))
    }).collect()
}

/// For a given rectangle, determines what text alignment should be used
fn determine_text_alignment(rect: &StyledNode)
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


/// Returns all node IDs where the children overflow the parent, together with the
/// `(parent_rect, child_rect)` - the child rect is the sum of the children.
///
/// TODO: The performance of this function can be theoretically improved:
///
/// - Unioning the rectangles is heavier than just looping through the children and
/// summing up their width / height / padding + margin.
/// - Scroll nodes only need to be inserted if the parent doesn't have `overflow: hidden`
/// activated
/// - Overflow for X and Y needs to be tracked seperately (for overflow-x / overflow-y separation),
/// so there we'd need to track in which direction the inner_rect is overflowing.
fn get_nodes_that_need_scroll_clip(
    display_list_rects: &NodeDataContainerRef<StyledNode>,
    dom_rects: &NodeDataContainerRef<NodeData>,
    layouted_rects: &NodeDataContainerRef<PositionedRectangle>,
    parents: &[ParentWithNodeDepth],
    pipeline_id: PipelineId,
) -> ScrolledNodes {

    use azul_core::ui_solver::{DirectionalOverflowInfo, OverflowingScrollNode, ExternalScrollId};
    use azul_core::dom::ScrollTagId;

    let mut nodes = BTreeMap::new();
    let mut tags_to_node_ids = BTreeMap::new();

    for ParentWithNodeDepth { depth, node_id } in parents.iter() {

        let parent_id = match node_id.into_crate_internal() { Some(s) => s, None => continue, };
        let parent_rect = &layouted_rects[parent_id];
        let overflow_x = &parent_rect.overflow.overflow_x;
        let overflow_y = &parent_rect.overflow.overflow_y;

        let scroll_rect = if overflow_x.is_none() || overflow_x.is_negative() {
            // no overflow in horizontal direction
            if overflow_y.is_none() || overflow_y.is_negative() {
                // no overflow in both directions
                None
            } else {
                // overflow in y, but not in x direction
                match overflow_y {
                    DirectionalOverflowInfo::Scroll { amount: Some(s) } | DirectionalOverflowInfo::Auto { amount: Some(s) } => {
                        Some(LayoutRect::new(LayoutPoint::zero(), LayoutSize::new(parent_rect.size.width, parent_rect.size.height + *s)))
                    },
                    _ => None
                }
            }
        } else {
            if overflow_y.is_none() || overflow_y.is_negative() {
                // overflow in x, but not in y direction
                match overflow_x {
                    DirectionalOverflowInfo::Scroll { amount: Some(s) } | DirectionalOverflowInfo::Auto { amount: Some(s) } => {
                        Some(LayoutRect::new(LayoutPoint::zero(), LayoutSize::new(parent_rect.size.width + *s, parent_rect.size.height)))
                    },
                    _ => None
                }
            } else {
                // overflow in x and y direction
                match overflow_x {
                    DirectionalOverflowInfo::Scroll { amount: Some(q) } | DirectionalOverflowInfo::Auto { amount: Some(q) } => {
                        match overflow_y {
                            DirectionalOverflowInfo::Scroll { amount: Some(s) } | DirectionalOverflowInfo::Auto { amount: Some(s) } => {
                                Some(LayoutRect::new(LayoutPoint::zero(), LayoutSize::new(parent_rect.size.width + *q, parent_rect.size.height + *s)))
                            },
                            _ => None
                        }
                    },
                    _ => None
                }
            }
        };

        let scroll_rect = match scroll_rect {
            Some(s) => s,
            None => continue,
        };

        let parent_dom_hash = dom_rects[parent_id].calculate_node_data_hash();

        // Create an external scroll id. This id is required to preserve its
        // scroll state accross multiple frames.
        let parent_external_scroll_id  = ExternalScrollId(parent_dom_hash.0, pipeline_id);

        // Create a unique scroll tag for hit-testing
        let scroll_tag_id = match display_list_rects.get(parent_id).and_then(|node| node.tag_id.into_option().map(|t| t.into_crate_internal())) {
            Some(existing_tag) => ScrollTagId(existing_tag),
            None => ScrollTagId::new(),
        };

        tags_to_node_ids.insert(scroll_tag_id, *node_id);
        nodes.insert(*node_id, OverflowingScrollNode {
            child_rect: scroll_rect,
            parent_external_scroll_id,
            parent_dom_hash,
            scroll_tag_id,
        });
    }

    ScrolledNodes { overflowing_nodes: nodes, tags_to_node_ids }
}

// Since there can be a small floating point error, round the item to the nearest pixel,
// then compare the rects
fn contains_rect_rounded(a: &LayoutRect, b: LayoutRect) -> bool {
    let a_x = a.origin.x.round() as isize;
    let a_y = a.origin.x.round() as isize;
    let a_width = a.size.width.round() as isize;
    let a_height = a.size.height.round() as isize;

    let b_x = b.origin.x.round() as isize;
    let b_y = b.origin.x.round() as isize;
    let b_width = b.size.width.round() as isize;
    let b_height = b.size.height.round() as isize;

    b_x >= a_x &&
    b_y >= a_y &&
    b_x + b_width <= a_x + a_width &&
    b_y + b_height <= a_y + a_height
}

fn node_needs_to_clip_children(layout: &RectLayout) -> bool {
    !(layout.is_horizontal_overflow_visible() || layout.is_vertical_overflow_visible())
}