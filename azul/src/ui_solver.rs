use std::{f32, collections::BTreeMap};
use azul_css::{
    LayoutPosition, LayoutMargin, LayoutPadding,
    RectLayout, StyleFontSize, RectStyle,
    StyleTextAlignmentHorz, StyleTextAlignmentVert, PixelValue,
};
use {
    id_tree::{NodeId, NodeDataContainer, NodeHierarchy},
    display_list::DisplayRectangle,
    dom::{NodeData, NodeType},
    app_resources::AppResources,
    text_layout::{Words, ScaledWords, TextLayoutOptions, WordPositions, LayoutedGlyphs},
};
use azul_core::{
    app_resources::{Au, FontInstanceKey},
};
use webrender::api::{LayoutRect, LayoutPoint, LayoutSize};

const DEFAULT_FLEX_GROW_FACTOR: f32 = 1.0;
const DEFAULT_FONT_SIZE: StyleFontSize = StyleFontSize(PixelValue::const_px(10));
const DEFAULT_FONT_ID: &str = "sans-serif";

type PixelSize = f32;

#[derive(Debug, Copy, Clone, PartialEq)]
enum WhConstraint {
    /// between min, max
    Between(f32, f32),
    /// Value needs to be exactly X
    EqualTo(f32),
    /// Value can be anything
    Unconstrained,
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
    fn $fn_name(layout: &RectLayout, preferred_inner_width: Option<f32>) -> WhConstraint {

        let width = layout.$width.map(|w| w.0.to_pixels());
        let min_width = layout.$min_width.map(|w| w.0.to_pixels());
        let max_width = layout.$max_width.map(|w| w.0.to_pixels());

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
            if let Some(width) = preferred_inner_width {
                // -- same as the width() block: width takes precedence over
                // no width, only min_width and max_width
                if let Some(max_width) = absolute_max {
                    if let Some(min_width) = absolute_min {
                        if min_width < width && width < max_width {
                            // normal: min_width < width < max_width
                            WhConstraint::Between(width, max_width)
                        } else if width > max_width {
                            WhConstraint::EqualTo(max_width)
                        } else if width < min_width {
                            WhConstraint::EqualTo(min_width)
                        } else {
                            WhConstraint::Unconstrained /* unreachable */
                        }
                    } else {
                        // width & max_width
                        let min = width.min(max_width);
                        let max = width.max(max_width);
                        WhConstraint::Between(min, max)
                    }
                } else if let Some(min_width) = absolute_min {
                    // no max width, only width & min_width
                    let min = width.min(min_width);
                    let max = width.max(min_width);
                    WhConstraint::Between(min, max)
                } else {
                    // no min-width or max-width
                    WhConstraint::Between(width, f32::MAX)
                }
            } else {
                // no width, no preferred width,
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

#[derive(Debug, Copy, Clone, PartialEq)]
struct WidthCalculatedRect {
    pub preferred_width: WhConstraint,
    pub margin: LayoutMargin,
    pub padding: LayoutPadding,
    pub flex_grow_px: f32,
    pub min_inner_size_px: f32,
}

impl WidthCalculatedRect {
    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_horizontal(&self) -> f32 {
        self.preferred_width.min_needed_space().unwrap_or(0.0)      +
        self.margin.left.map(|px| px.to_pixels()).unwrap_or(0.0)    +
        self.margin.right.map(|px| px.to_pixels()).unwrap_or(0.0)   +
        self.padding.left.map(|px| px.to_pixels()).unwrap_or(0.0)   +
        self.padding.right.map(|px| px.to_pixels()).unwrap_or(0.0)
    }

    /// Get the sum of the horizontal padding amount (`padding.left + padding.right`)
    pub fn get_horizontal_padding(&self) -> f32 {
        self.padding.left.map(|px| px.to_pixels()).unwrap_or(0.0)   +
        self.padding.right.map(|px| px.to_pixels()).unwrap_or(0.0)
    }

    /// Called after solver has run: Solved width of rectangle
    pub fn solved_result(&self) -> WidthSolvedResult {
        WidthSolvedResult {
            min_width: self.min_inner_size_px,
            space_added: self.flex_grow_px,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
struct HeightCalculatedRect {
    pub preferred_height: WhConstraint,
    pub margin: LayoutMargin,
    pub padding: LayoutPadding,
    pub flex_grow_px: f32,
    pub min_inner_size_px: f32,
}

impl HeightCalculatedRect {
    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_vertical(&self) -> f32 {
        self.preferred_height.min_needed_space().unwrap_or(0.0) +
        self.margin.top.map(|px| px.to_pixels()).unwrap_or(0.0) +
        self.margin.bottom.map(|px| px.to_pixels()).unwrap_or(0.0) +
        self.padding.top.map(|px| px.to_pixels()).unwrap_or(0.0) +
        self.padding.bottom.map(|px| px.to_pixels()).unwrap_or(0.0)
    }

    /// Get the sum of the horizontal padding amount (`padding.top + padding.bottom`)
    pub fn get_vertical_padding(&self) -> f32 {
        self.padding.top.map(|px| px.to_pixels()).unwrap_or(0.0) +
        self.padding.bottom.map(|px| px.to_pixels()).unwrap_or(0.0)
    }

    /// Called after solver has run: Solved width of rectangle
    pub fn solved_result(&self) -> HeightSolvedResult {
        HeightSolvedResult {
            min_height: self.min_inner_size_px,
            space_added: self.flex_grow_px,
        }
    }
}

// `typed_arena!(WidthCalculatedRect, preferred_width, determine_preferred_width, get_horizontal_padding, get_flex_basis_horizontal)`
macro_rules! typed_arena {(
    $module_name:ident,
    $struct_name:ident,
    $preferred_field:ident,
    $determine_preferred_fn:ident,
    $get_padding_fn:ident,
    $get_flex_basis:ident,
    $bubble_fn_name:ident,
    $main_axis:ident
) => (

mod $module_name  {

    use super::*;

    /// Fill out the preferred width of all nodes.
    ///
    /// We could operate on the Arena<DisplayRectangle> directly, but that makes testing very
    /// hard since we are only interested in testing or touching the layout. So this makes the
    /// calculation maybe a few microseconds slower, but gives better testing capabilities
    ///
    /// NOTE: Later on, this could maybe be a NodeDataContainer<&'a RectLayout>.
    #[must_use]
    pub(in super) fn from_rect_layout_arena(node_data: &NodeDataContainer<RectLayout>, widths: &NodeDataContainer<Option<f32>>) -> NodeDataContainer<$struct_name> {
        let new_nodes = node_data.internal.iter().enumerate().map(|(node_id, node_data)|{
            let id = NodeId::new(node_id);
            $struct_name {
                // TODO: get the initial width of the rect content
                $preferred_field: $determine_preferred_fn(&node_data, widths[id]),
                margin: node_data.margin.unwrap_or_default(),
                padding: node_data.padding.unwrap_or_default(),
                flex_grow_px: 0.0,
                min_inner_size_px: 0.0,
            }
        }).collect();
        NodeDataContainer { internal: new_nodes }
    }

    /// Bubble the inner sizes to their parents -  on any parent nodes, fill out
    /// the width so that the `preferred_width` can contain the child nodes (if
    /// that doesn't violate the constraints of the parent)
    pub(in super) fn $bubble_fn_name(
        node_width_container: &mut NodeDataContainer<$struct_name>,
        node_hierarchy: &NodeHierarchy,
        arena_data: &NodeDataContainer<RectLayout>,
        non_leaf_nodes: &[(usize, NodeId)])
    {
        // Reverse, since we want to go from the inside out (depth 5 needs to be filled out first)
        //
        // Set the preferred_width of the parent nodes
        for (_node_depth, non_leaf_id) in non_leaf_nodes.iter().rev() {

            use self::WhConstraint::*;

            // Sum of the direct children's flex-basis = the parents preferred width
            let children_flex_basis = sum_children_flex_basis(node_width_container, *non_leaf_id, node_hierarchy, arena_data);

            // Calculate the new flex-basis width
            let parent_width_metrics = node_width_container[*non_leaf_id];

            // For calculating the inner width, subtract the parents padding
            let parent_padding = node_width_container[*non_leaf_id].$get_padding_fn();

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

            node_width_container[*non_leaf_id].min_inner_size_px = child_width;
        }

        // Now, the width of all elements should be filled,
        // but they aren't flex-growed or flex-shrinked yet
    }

    /// Go from the root down and flex_grow the children if needed - respects the `width`, `min_width` and `max_width` properties
    /// The layout step doesn't account for the min_width and max_width constraints, so we have to adjust them manually
    pub(in super) fn apply_flex_grow(
        node_width_container: &mut NodeDataContainer<$struct_name>,
        node_hierarchy: &NodeHierarchy,
        arena_data: &NodeDataContainer<RectLayout>,
        parent_ids_sorted_by_depth: &[(usize, NodeId)],
        root_width: f32
    ) {
        use azul_css::LayoutAlignItems;

        debug_assert!(node_width_container[NodeId::new(0)].flex_grow_px == 0.0);

        // Set the window width on the root node (since there is only one root node, we can
        // calculate the `flex_grow_px` directly)
        //
        // Usually `top_level_flex_basis` is NOT 0.0, rather it's the sum of all widths in the DOM,
        // i.e. the sum of the whole DOM tree
        let top_level_flex_basis = node_width_container[NodeId::new(0)].min_inner_size_px;

        // The root node can still have some sort of max-width attached, so we need to check for that
        let root_preferred_width = if let Some(max_width) = node_width_container[NodeId::new(0)].$preferred_field.max_available_space() {
            if root_width > max_width { max_width } else { root_width }
        } else {
            root_width
        };

        node_width_container[NodeId::new(0)].flex_grow_px = root_preferred_width - top_level_flex_basis;

        // Keep track of the nearest relative or absolute positioned element
        let mut positioned_node_stack = vec![NodeId::new(0)];

        for (_node_depth, parent_id) in parent_ids_sorted_by_depth {

            use azul_css::{LayoutAxis, LayoutPosition};

            let parent_node = &arena_data[*parent_id];
            let parent_is_positioned = parent_node.position.unwrap_or_default() != LayoutPosition::Static;

            if parent_is_positioned {
                positioned_node_stack.push(*parent_id);
            }

            // How much width is there to distribute along the main and cross axis?
            let (width_main_axis, width_cross_axis) = {
                let parent_width_metrics = &node_width_container[*parent_id];

                let width_horizontal_axis = {
                    let children_margin: f32 = parent_id.children(node_hierarchy).map(|child_id| arena_data[child_id].get_horizontal_margin()).sum();
                    parent_width_metrics.min_inner_size_px + parent_width_metrics.flex_grow_px - parent_node.get_horizontal_padding() - children_margin
                };

                let width_vertical_axis = {
                    // let children_margin: f32 = parent_id.children(node_hierarchy).map(|child_id| arena_data[child_id].get_vertical_margin()).sum();
                    parent_width_metrics.min_inner_size_px + parent_width_metrics.flex_grow_px - parent_node.get_vertical_padding()
                };

                let width_main_axis = match LayoutAxis::$main_axis {
                    LayoutAxis::Horizontal => width_horizontal_axis,
                    LayoutAxis::Vertical => width_vertical_axis,
                };

                let width_cross_axis = match LayoutAxis::$main_axis {
                    LayoutAxis::Horizontal => width_vertical_axis,
                    LayoutAxis::Vertical => width_horizontal_axis,
                };

                (width_main_axis, width_cross_axis)
            };

            // Only stretch the items, if they have a align-items: stretch!
            if parent_node.align_items.unwrap_or_default() == LayoutAlignItems::Stretch {
                if parent_node.direction.unwrap_or_default().get_axis() == LayoutAxis::$main_axis {
                    distribute_space_along_main_axis(parent_id, width_main_axis, node_hierarchy, arena_data, node_width_container, &positioned_node_stack);
                } else {
                    distribute_space_along_cross_axis(parent_id, width_cross_axis, node_hierarchy, arena_data, node_width_container, &positioned_node_stack);
                }
            }

            if parent_is_positioned {
                positioned_node_stack.pop();
            }
        }
    }

    /// Returns the sum of the flex-basis of the current nodes' children
    pub(in super) fn sum_children_flex_basis(
        node_width_container: &NodeDataContainer<$struct_name>,
        node_id: NodeId,
        node_hierarchy: &NodeHierarchy,
        display_arena: &NodeDataContainer<RectLayout>)
    -> f32
    {
        node_id
            .children(node_hierarchy)
            .filter(|child_node_id| display_arena[*child_node_id].position != Some(LayoutPosition::Absolute))
            .map(|child_node_id| node_width_container[child_node_id].$get_flex_basis())
            .sum()
    }

    /// Does the actual width layout, respects the `width`, `min_width` and `max_width`
    /// properties as well as the `flex_grow` factor. `flex_shrink` currently does nothing.
    pub(in super) fn distribute_space_along_main_axis(
        node_id: &NodeId,
        width_to_distribute: f32,
        node_hierarchy: &NodeHierarchy,
        arena_data: &NodeDataContainer<RectLayout>,
        width_calculated_arena: &mut NodeDataContainer<$struct_name>,
        positioned_node_stack: &[NodeId]
    ) {
        let mut parent_node_inner_width = width_to_distribute;

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
                if arena_data[exact_width_child_id].position.unwrap_or_default() != LayoutPosition::Absolute {
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

        use FastHashSet;

        let mut variable_width_childs = node_id
            .children(node_hierarchy)
            .filter(|id| !width_calculated_arena[*id].$preferred_field.is_fixed_constraint())
            .collect::<FastHashSet<NodeId>>();

        let mut absolute_variable_width_nodes = Vec::new();

        for variable_child_id in &variable_width_childs {

            if arena_data[*variable_child_id].position.unwrap_or_default() != LayoutPosition::Absolute {

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
                            .map(|grow| grow.0.get().max(1.0))
                            .unwrap_or(DEFAULT_FLEX_GROW_FACTOR))
                .sum();

            // Grow all variable children by the same amount.
            for variable_child_id in &variable_width_childs {

                let flex_grow = arena_data[*variable_child_id].flex_grow
                    .and_then(|grow| Some(grow.0.get()))
                    .unwrap_or(DEFAULT_FLEX_GROW_FACTOR);

                // Do not expand the item on "flex-grow: 0" (prevent division by 0)
                if flex_grow as usize == 0 {
                    continue;
                }

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

    pub(in super) fn distribute_space_along_cross_axis(
        node_id: &NodeId,
        width_to_distribute: f32,
        node_hierarchy: &NodeHierarchy,
        arena_data: &NodeDataContainer<RectLayout>,
        width_calculated_arena: &mut NodeDataContainer<$struct_name>,
        positioned_node_stack: &[NodeId])
    {
        let parent_node_inner_width = width_to_distribute;

        let last_relative_node_width = {
            let zero_node = NodeId::new(0);
            let last_relative_node_id = positioned_node_stack.get(positioned_node_stack.len() - 1).unwrap_or(&zero_node);
            let last_relative_node = &width_calculated_arena[*last_relative_node_id];
            last_relative_node.min_inner_size_px + last_relative_node.flex_grow_px - last_relative_node.$get_padding_fn()
        };

        for child_id in node_id.children(node_hierarchy) {

            let parent_node_inner_width = if arena_data[child_id].position.unwrap_or_default() != LayoutPosition::Absolute {
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
}

)}

typed_arena!(
    solve_width,
    WidthCalculatedRect,
    preferred_width,
    determine_preferred_width,
    get_horizontal_padding,
    get_flex_basis_horizontal,
    bubble_preferred_widths_to_parents,
    Horizontal
);

typed_arena!(
    solve_height,
    HeightCalculatedRect,
    preferred_height,
    determine_preferred_height,
    get_vertical_padding,
    get_flex_basis_vertical,
    bubble_preferred_heights_to_parents,
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
pub(crate) fn solve_flex_layout_width<'a>(
    node_hierarchy: &NodeHierarchy,
    display_rectangles: &NodeDataContainer<DisplayRectangle<'a>>,
    preferred_widths: &NodeDataContainer<Option<f32>>,
    window_width: f32
) -> SolvedWidthLayout {
    let layout_only_arena = display_rectangles.transform(|node, _| node.layout);
    let mut width_calculated_arena = solve_width::from_rect_layout_arena(&layout_only_arena, preferred_widths);
    let non_leaf_nodes_sorted_by_depth = node_hierarchy.get_parents_sorted_by_depth();
    solve_width::bubble_preferred_widths_to_parents(&mut width_calculated_arena, node_hierarchy, &layout_only_arena, &non_leaf_nodes_sorted_by_depth);
    solve_width::apply_flex_grow(&mut width_calculated_arena, node_hierarchy, &layout_only_arena, &non_leaf_nodes_sorted_by_depth, window_width);
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
    let mut height_calculated_arena = solve_height::from_rect_layout_arena(&layout_only_arena, preferred_heights);
    solve_height::bubble_preferred_heights_to_parents(&mut height_calculated_arena, node_hierarchy, &layout_only_arena, &solved_widths.non_leaf_nodes_sorted_by_depth);
    solve_height::apply_flex_grow(&mut height_calculated_arena, node_hierarchy, &layout_only_arena, &solved_widths.non_leaf_nodes_sorted_by_depth, window_height);
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
    ) {
        let child_width_with_padding = {
            let child_node = &solved_widths.$solved_widths_field[child_id];
            child_node.$min_width + child_node.space_added
        };

        let child_node = &arena_data[child_id];
        let child_margin = child_node.margin.unwrap_or_default();
        let child_margin_left = child_margin.$left.map(|x| x.to_pixels()).unwrap_or(0.0);
        let child_margin_right = child_margin.$right.map(|x| x.to_pixels()).unwrap_or(0.0);

        let zero_node = NodeId::new(0);
        let last_relative_node_id = positioned_node_stack.get(positioned_node_stack.len() - 1).unwrap_or(&zero_node);

        let last_relative_node = arena_data[*last_relative_node_id];
        let last_relative_padding = last_relative_node.padding.unwrap_or_default();
        let last_relative_padding_left = last_relative_padding.$left.map(|x| x.to_pixels()).unwrap_or(0.0);
        let last_relative_padding_right = last_relative_padding.$right.map(|x| x.to_pixels()).unwrap_or(0.0);

        let last_relative_node_x = arena_solved_data[*last_relative_node_id].0 + last_relative_padding_left;
        let last_relative_node_inner_width = {
            let last_relative_node = &solved_widths.$solved_widths_field[*last_relative_node_id];
            last_relative_node.$min_width + last_relative_node.space_added - (last_relative_padding_left + last_relative_padding_right)
        };

        let child_left = &arena_data[child_id].$left.map(|s| s.0.to_pixels());
        let child_right = &arena_data[child_id].$right.map(|s| s.0.to_pixels());

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
        parent_id: NodeId,
        main_axis_alignment: LayoutJustifyContent,
        arena_data: &NodeDataContainer<RectLayout>,
        arena_solved_data: &mut NodeDataContainer<$height_solved_position>,
        solved_widths: &$width_layout,
        child_id: NodeId,
        parent_x_position: f32,
        parent_inner_width: f32,
        sum_x_of_children_so_far: &mut f32,
        positioned_node_stack: &[NodeId],
    ) {
        use azul_css::LayoutJustifyContent::*;

        let child_width_with_padding = {
            let child_node = &solved_widths.$solved_widths_field[child_id];
            child_node.$min_width + child_node.space_added
        };

        // width: increase X according to the main axis, Y according to the cross_axis
        let child_node = &arena_data[child_id];
        let child_margin = child_node.margin.unwrap_or_default();
        let child_margin_left = child_margin.$left.map(|x| x.to_pixels()).unwrap_or(0.0);
        let child_margin_right = child_margin.$right.map(|x| x.to_pixels()).unwrap_or(0.0);

        if child_node.position.unwrap_or_default() == LayoutPosition::Absolute {
            determine_child_x_absolute(
                child_id,
                positioned_node_stack,
                arena_data,
                arena_solved_data,
                solved_widths
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
        parent_x_position: f32)
    {
        let child_node = &arena_data[child_id];
        let child_margin_left = child_node.margin.unwrap_or_default().$left.map(|x| x.to_pixels()).unwrap_or(0.0);

        if child_node.position.unwrap_or_default() == LayoutPosition::Absolute {
            determine_child_x_absolute(
                child_id,
                positioned_node_stack,
                arena_data,
                arena_solved_data,
                solved_widths
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

        let parent_padding = parent_node.padding.unwrap_or_default();
        let parent_padding_left = parent_padding.$left.map(|x| x.to_pixels()).unwrap_or(0.0);
        let parent_padding_right = parent_padding.$right.map(|x| x.to_pixels()).unwrap_or(0.0);

        let parent_x_position = arena_solved_data[*parent_id].0 + parent_padding_left;
        let parent_direction = parent_node.direction.unwrap_or_default();

        // Push nearest relative or absolute positioned element
        let parent_is_positioned = parent_node.position.unwrap_or_default() != LayoutPosition::Static;
        if parent_is_positioned {
            positioned_node_stack.push(*parent_id);
        }

        let parent_inner_width = {
            let parent_node = &solved_widths.$solved_widths_field[*parent_id];
            parent_node.$min_width + parent_node.space_added - (parent_padding_left + parent_padding_right)
        };

        if parent_direction.get_axis() == LayoutAxis::$axis {
            // Along main axis: Take X of parent
            let main_axis_alignment = node_data[*parent_id].justify_content.unwrap_or_default();
            let mut sum_x_of_children_so_far = 0.0;

            if parent_direction.is_reverse() {
                for child_id in parent_id.reverse_children(node_hierarchy) {
                    determine_child_x_along_main_axis(
                        *parent_id,
                        main_axis_alignment,
                        &node_data,
                        &mut arena_solved_data,
                        solved_widths,
                        child_id,
                        parent_x_position,
                        parent_inner_width,
                        &mut sum_x_of_children_so_far,
                        &positioned_node_stack,
                    );
                }
            } else {
                for child_id in parent_id.children(node_hierarchy) {
                    determine_child_x_along_main_axis(
                        *parent_id,
                        main_axis_alignment,
                        &node_data,
                        &mut arena_solved_data,
                        solved_widths,
                        child_id,
                        parent_x_position,
                        parent_inner_width,
                        &mut sum_x_of_children_so_far,
                        &positioned_node_stack,
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
                    node_data[*ch].position.unwrap_or_default() != LayoutPosition::Absolute
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
    get_position!(get_pos_x, SolvedWidthLayout, HorizontalSolvedPosition, solved_widths, min_width, left, right, Horizontal);
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
    get_position!(get_pos_y, SolvedHeightLayout, VerticalSolvedPosition, solved_heights, min_height, top, bottom, Vertical);
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
fn get_content_width<T>(
        node_id: &NodeId,
        node_type: &NodeType<T>,
        app_resources: &AppResources,
        positioned_words: &BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
) -> Option<f32> {
    use dom::NodeType::*;
    match node_type {
        Image(image_id) => app_resources.get_image_info(image_id).map(|info| info.descriptor.dimensions.0 as f32),
        Label(_) | Text(_) => positioned_words.get(node_id).map(|pos| pos.0.content_size.width),
        _ => None,
    }
}

fn get_content_height<T>(
    node_id: &NodeId,
    node_type: &NodeType<T>,
    app_resources: &AppResources,
    positioned_words: &BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    div_width: f32,
) -> Option<PreferredHeight> {
    use dom::NodeType::*;
    match &node_type {
        Image(i) => {
            let (image_size_width, image_size_height) = app_resources.get_image_info(i)?.descriptor.dimensions;
            let aspect_ratio = image_size_width as f32 / image_size_height as f32;
            let preferred_height = div_width * aspect_ratio;
            Some(PreferredHeight::Image {
                original_dimensions: (image_size_width, image_size_height),
                aspect_ratio,
                preferred_height,
            })
        },
        Label(_) | Text(_) => {
            positioned_words
            .get(node_id)
            .map(|pos| PreferredHeight::Text { content_size: pos.0.content_size })
        },
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PreferredHeight {
    Image { original_dimensions: (usize, usize), aspect_ratio: f32, preferred_height: f32 },
    Text { content_size: LayoutSize }
}

impl PreferredHeight {

    /// Returns the preferred size of the div content.
    /// Note that this can be larger than the actual div content!
    pub fn get_content_size(&self) -> f32 {
        use self::PreferredHeight::*;
        match self {
            Image { preferred_height, .. } => *preferred_height,
            Text { content_size } => content_size.height,
        }
    }
}

pub(crate) fn font_size_to_au(font_size: StyleFontSize) -> Au {
    px_to_au(font_size.0.to_pixels())
}

pub(crate) fn px_to_au(px: f32) -> Au {
    use app_units::{Au as WrAu, AU_PER_PX, MIN_AU, MAX_AU};

    let target_app_units = WrAu((px * AU_PER_PX as f32) as i32);
    Au(target_app_units.min(MAX_AU).max(MIN_AU).0)
}

pub(crate) fn get_font_id(rect_style: &RectStyle) -> &str {
    let font_id = rect_style.font_family.as_ref().and_then(|family| family.fonts.get(0));
    font_id.map(|f| f.get_str()).unwrap_or(DEFAULT_FONT_ID)
}

pub(crate) fn get_font_size(rect_style: &RectStyle) -> StyleFontSize {
    rect_style.font_size.unwrap_or(DEFAULT_FONT_SIZE)
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct PositionedRectangle {
    pub bounds: LayoutRect,
    /// Size of the content, for example if a div contains an image,
    /// that image can be bigger than the actual rect
    pub content_width: Option<f32>,
    pub content_height: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct LayoutResult {
    pub rects: NodeDataContainer<PositionedRectangle>,
    pub word_cache: BTreeMap<NodeId, Words>,
    pub scaled_words: BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
    pub positioned_word_cache: BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    pub layouted_glyph_cache: BTreeMap<NodeId, LayoutedGlyphs>,
    pub node_depths: Vec<(usize, NodeId)>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
struct InlineText {
    /// Horizontal padding of the text in pixels
    horizontal_padding: f32,
    /// Horizontal margin of the text in pixels
    horizontal_margin: f32,
}

/// At this point in time, all font keys, image keys, etc. have
/// to be already submitted in the RenderApi!
pub(crate) fn do_the_layout<'a,'b, T>(
    node_hierarchy: &NodeHierarchy,
    node_data: &NodeDataContainer<NodeData<T>>,
    display_rects: &NodeDataContainer<DisplayRectangle<'a>>,
    app_resources: &'b AppResources,
    rect_size: LayoutSize,
    rect_offset: LayoutPoint,
) -> LayoutResult {

    // Determine what the width for each div would be if the content size didn't matter
    let widths_content_ignored = solve_flex_layout_width(
        node_hierarchy,
        &display_rects,
        &node_data.transform(|_, _| None),
        rect_size.width as f32,
    );

    // Determine what the "maximum width" for each div is, except for divs where overflow:visible is set
    // I.e. for a div width 800px, with 4 text child nodes, each text node gets a width of 200px
    let max_widths = node_hierarchy
        .linear_iter()
        .filter(|node_id| !display_rects[*node_id].layout.is_horizontal_overflow_visible())
        .map(|node_id| (node_id, widths_content_ignored.solved_widths[node_id].total()))
        .collect::<BTreeMap<NodeId, f32>>();

    // TODO: Filter all inline text blocks (prepare inline text layout run)
    let inline_text_blocks = BTreeMap::<NodeId, InlineText>::new();

    // Resolve cached text IDs or break new, uncached strings into words / text runs
    let word_cache = create_word_cache(app_resources, node_data);
    // Scale the words to the correct size - TODO: Caching / GC!
    let scaled_words = create_scaled_words(app_resources, &word_cache, display_rects);
    // Layout all words as if there was no max-width constraint
    let word_positions_no_max_width = create_word_positions(
        &word_cache,
        &scaled_words,
        display_rects,
        &max_widths,
        &inline_text_blocks
    );

    // Determine the preferred **content** width, without any max-width restrictions -
    // For images that would be the image width / height, for text it would be the text
    // laid out without any width constraints.
    let content_widths = node_data.transform(|node, node_id|
        get_content_width(&node_id, &node.get_node_type(), app_resources, &word_positions_no_max_width)
    );

    // Solve the widths again, this time incorporating the maximum widths
    let solved_widths = solve_flex_layout_width(
        node_hierarchy,
        &display_rects,
        &content_widths,
        rect_size.width as f32,
    );

    // Layout all texts again with the resolved width constraints
    let proper_max_widths = solved_widths.solved_widths.linear_iter().map(|node_id| {
        (node_id, solved_widths.solved_widths[node_id].total() - display_rects[node_id].layout.get_horizontal_padding())
    }).collect();

    // Resolve the word positions relative to each divs upper left corner
    let word_positions_with_max_width = create_word_positions(
        &word_cache,
        &scaled_words,
        display_rects,
        &proper_max_widths,
        &inline_text_blocks
    );

    // Given the final width of a node and the height of the content, resolve the div
    // height and return whether the node content overflows its parent (width-in-height-out)
    let content_heights = node_data.transform(|node, node_id| {
        let div_width = solved_widths.solved_widths[node_id].total();
        get_content_height(
            &node_id,
            node.get_node_type(),
            app_resources,
            &word_positions_with_max_width,
            div_width
        ).map(|ch| ch.get_content_size())
    });

    // Given the final heights, resolve the heights for flexible-size divs
    // TODO: Fix justify-content:flex-start: The content height is not the final height!
    let solved_heights = solve_flex_layout_height(
        node_hierarchy,
        &solved_widths,
        &content_heights,
        rect_size.height as f32,
    );

    let x_positions = get_x_positions(&solved_widths, node_hierarchy, rect_offset.clone());
    let y_positions = get_y_positions(&solved_heights, &solved_widths, node_hierarchy, rect_offset);

    let layouted_rects = node_data.transform(|_node, node_id| {
        PositionedRectangle {
            bounds: LayoutRect::new(
                LayoutPoint::new(x_positions[node_id].0, y_positions[node_id].0),
                LayoutSize::new(
                    solved_widths.solved_widths[node_id].total(),
                    solved_heights.solved_heights[node_id].total(),
                )
            ),
            content_width: Some(proper_max_widths[&node_id]),
            content_height: content_heights[node_id],
        }
    });

    let positioned_word_cache = word_positions_with_max_width;

    // Create and layout the actual glyphs (important for actually )
    let layouted_glyph_cache = get_glyphs(
        &scaled_words,
        &positioned_word_cache,
        &display_rects,
        &layouted_rects,
    );

    LayoutResult {
        rects: layouted_rects,
        word_cache,
        scaled_words,
        positioned_word_cache,
        layouted_glyph_cache,
        node_depths: solved_widths.non_leaf_nodes_sorted_by_depth,
    }
}

fn create_word_cache<T>(
    app_resources: &AppResources,
    node_data: &NodeDataContainer<NodeData<T>>,
) -> BTreeMap<NodeId, Words> {
    use text_layout::split_text_into_words;
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

fn create_scaled_words<'a>(
    app_resources: &AppResources,
    words: &BTreeMap<NodeId, Words>,
    display_rects: &NodeDataContainer<DisplayRectangle<'a>>,
) -> BTreeMap<NodeId, (ScaledWords, FontInstanceKey)> {

    use text_layout::words_to_scaled_words;
    use app_resources::ImmediateFontId;

    words.iter().filter_map(|(node_id, words)| {
        let style = &display_rects[*node_id].style;
        let font_size = get_font_size(&style);
        let font_size_au = font_size_to_au(font_size);
        let css_font_id = get_font_id(&style);
        let font_id = match app_resources.get_css_font_id(css_font_id) {
            Some(s) => ImmediateFontId::Resolved(*s),
            None => ImmediateFontId::Unresolved(css_font_id.to_string()),
        };

        let loaded_font = app_resources.get_loaded_font(&font_id)?;
        let font_instance_key = loaded_font.font_instances.get(&font_size_au)?;

        let font_bytes = &loaded_font.font_bytes;
        let font_index = loaded_font.font_index as u32;

        let scaled_words = words_to_scaled_words(
            words,
            font_bytes,
            font_index,
            font_size.0.to_pixels(),
        );
        Some((*node_id, (scaled_words, *font_instance_key)))
    }).collect()
}

fn create_word_positions<'a>(
    words: &BTreeMap<NodeId, Words>,
    scaled_words: &BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
    display_rects: &NodeDataContainer<DisplayRectangle<'a>>,
    max_widths: &BTreeMap<NodeId, PixelSize>,
    inline_texts: &BTreeMap<NodeId, InlineText>,
) -> BTreeMap<NodeId, (WordPositions, FontInstanceKey)> {

    use text_layout;

    words.iter().filter_map(|(node_id, words)| {

        let rect = &display_rects[*node_id];
        let (scaled_words, font_instance_key) = scaled_words.get(&node_id)?;

        let font_size = get_font_size(&rect.style).0;
        let max_horizontal_width = max_widths.get(&node_id).cloned();
        let leading = inline_texts.get(&node_id).map(|inline_text| inline_text.horizontal_margin + inline_text.horizontal_padding);

        // TODO: Make this configurable
        let text_holes = Vec::new();
        let text_layout_options = get_text_layout_options(&rect, max_horizontal_width, leading, text_holes);

        // TODO: handle overflow / scrollbar_style !
        let positioned_words = text_layout::position_words(
            words, scaled_words,
            &text_layout_options,
            font_size.to_pixels()
        );

        Some((*node_id, (positioned_words, *font_instance_key)))
    }).collect()
}

fn get_text_layout_options(
    rect: &DisplayRectangle,
    max_horizontal_width: Option<f32>,
    leading: Option<f32>,
    holes: Vec<LayoutRect>,
) -> TextLayoutOptions {
    TextLayoutOptions {
        line_height: rect.style.line_height.map(|lh| lh.0.get()),
        letter_spacing: rect.style.letter_spacing.map(|ls| ls.0.to_pixels()),
        word_spacing: rect.style.word_spacing.map(|ws| ws.0.to_pixels()),
        tab_width: rect.style.tab_width.map(|tw| tw.0.get()),
        max_horizontal_width,
        leading,
        holes,
    }
}

fn get_glyphs<'a>(
    scaled_words: &BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
    positioned_word_cache: &BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    display_rects: &NodeDataContainer<DisplayRectangle<'a>>,
    positioned_rectangles: &NodeDataContainer<PositionedRectangle>,
) -> BTreeMap<NodeId, LayoutedGlyphs> {

    use text_layout::get_layouted_glyphs;

    scaled_words
    .iter()
    .filter_map(|(node_id, (scaled_words, _))| {

        let display_rect = display_rects.get(*node_id)?;
        let layouted_rect = positioned_rectangles.get(*node_id)?;
        let (word_positions, _) = positioned_word_cache.get(node_id)?;
        let (horz_alignment, vert_alignment) = determine_text_alignment(&display_rect.style, &display_rect.layout);

        let rect_padding_top = display_rect.layout.padding.unwrap_or_default().top.map(|top| top.to_pixels()).unwrap_or(0.0);
        let rect_padding_left = display_rect.layout.padding.unwrap_or_default().left.map(|left| left.to_pixels()).unwrap_or(0.0);
        let rect_offset = LayoutPoint::new(layouted_rect.bounds.origin.x + rect_padding_left, layouted_rect.bounds.origin.y + rect_padding_top);
        let bounding_size_height_px = layouted_rect.bounds.size.height - display_rect.layout.get_vertical_padding();

        Some((*node_id, get_layouted_glyphs(
            word_positions,
            scaled_words,
            horz_alignment,
            vert_alignment,
            rect_offset.clone(),
            bounding_size_height_px
        )))
    }).collect()
}

/// For a given rectangle, determines what text alignment should be used
fn determine_text_alignment(
    rect_style: &RectStyle,
    rect_layout: &RectLayout,
) -> (StyleTextAlignmentHorz, StyleTextAlignmentVert) {

    let mut horz_alignment = StyleTextAlignmentHorz::default();
    let mut vert_alignment = StyleTextAlignmentVert::default();

    if let Some(align_items) = rect_layout.align_items {
        // Vertical text alignment
        use azul_css::LayoutAlignItems;
        match align_items {
            LayoutAlignItems::Start => vert_alignment = StyleTextAlignmentVert::Top,
            LayoutAlignItems::End => vert_alignment = StyleTextAlignmentVert::Bottom,
            // technically stretch = blocktext, but we don't have that yet
            _ => vert_alignment = StyleTextAlignmentVert::Center,
        }
    }

    if let Some(justify_content) = rect_layout.justify_content {
        use azul_css::LayoutJustifyContent;
        // Horizontal text alignment
        match justify_content {
            LayoutJustifyContent::Start => horz_alignment = StyleTextAlignmentHorz::Left,
            LayoutJustifyContent::End => horz_alignment = StyleTextAlignmentHorz::Right,
            _ => horz_alignment = StyleTextAlignmentHorz::Center,
        }
    }

    if let Some(text_align) = rect_style.text_align {
        // Horizontal text alignment with higher priority
        horz_alignment = text_align;
    }

    (horz_alignment, vert_alignment)
}

#[cfg(test)]
mod layout_tests {

    use azul_css::RectLayout;
    use id_tree::{Node, NodeId};
    use super::*;

    /// Returns a DOM for testing so we don't have to construct it every time.
    /// The DOM structure looks like this:
    ///
    /// ```no_run
    /// 0
    /// '- 1
    ///    '-- 2
    ///    '   '-- 3
    ///    '   '--- 4
    ///    '-- 5
    /// ```
    fn get_testing_hierarchy() -> NodeHierarchy {
        NodeHierarchy {
            internal: vec![
                // 0
                Node {
                    parent: None,
                    previous_sibling: None,
                    next_sibling: None,
                    first_child: Some(NodeId::new(1)),
                    last_child: Some(NodeId::new(1)),
                },
                // 1
                Node {
                    parent: Some(NodeId::new(0)),
                    previous_sibling: None,
                    next_sibling: Some(NodeId::new(5)),
                    first_child: Some(NodeId::new(2)),
                    last_child: Some(NodeId::new(2)),
                },
                // 2
                Node {
                    parent: Some(NodeId::new(1)),
                    previous_sibling: None,
                    next_sibling: None,
                    first_child: Some(NodeId::new(3)),
                    last_child: Some(NodeId::new(4)),
                },
                // 3
                Node {
                    parent: Some(NodeId::new(2)),
                    previous_sibling: None,
                    next_sibling: Some(NodeId::new(4)),
                    first_child: None,
                    last_child: None,
                },
                // 4
                Node {
                    parent: Some(NodeId::new(2)),
                    previous_sibling: Some(NodeId::new(3)),
                    next_sibling: None,
                    first_child: None,
                    last_child: None,
                },
                // 5
                Node {
                    parent: Some(NodeId::new(1)),
                    previous_sibling: Some(NodeId::new(2)),
                    next_sibling: None,
                    first_child: None,
                    last_child: None,
                },
            ]
        }
    }

    /// Returns the same arena, but pre-fills nodes at [(NodeId, RectLayout)]
    /// with the layout rect
    fn get_display_rectangle_arena(constraints: &[(usize, RectLayout)]) -> (NodeHierarchy, NodeDataContainer<RectLayout>) {
        let arena = get_testing_hierarchy();
        let mut arena_data = vec![RectLayout::default(); arena.len()];
        for (id, rect) in constraints {
            arena_data[*id] = *rect;
        }
        (arena, NodeDataContainer { internal: arena_data })
    }

    #[test]
    fn test_determine_preferred_width() {
        use azul_css::{LayoutMinWidth, LayoutMaxWidth, PixelValue, LayoutWidth};

        let layout = RectLayout {
            width: None,
            min_width: None,
            max_width: None,
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::Unconstrained);

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(500.0))),
            min_width: None,
            max_width: None,
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(500.0));

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(500.0))),
            min_width: Some(LayoutMinWidth(PixelValue::px(600.0))),
            max_width: None,
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(600.0));

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(10000.0))),
            min_width: Some(LayoutMinWidth(PixelValue::px(600.0))),
            max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(800.0));

        let layout = RectLayout {
            width: None,
            min_width: Some(LayoutMinWidth(PixelValue::px(600.0))),
            max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::Between(600.0, 800.0));

        let layout = RectLayout {
            width: None,
            min_width: None,
            max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::Between(0.0, 800.0));

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(1000.0))),
            min_width: None,
            max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(800.0));

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(1200.0))),
            min_width: Some(LayoutMinWidth(PixelValue::px(1000.0))),
            max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(800.0));

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(1200.0))),
            min_width: Some(LayoutMinWidth(PixelValue::px(1000.0))),
            max_width: Some(LayoutMaxWidth(PixelValue::px(400.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(400.0));
    }

    /// Tests that the nodes get filled correctly
    #[test]
    fn test_fill_out_preferred_width() {

        use azul_css::*;

        let (node_hierarchy, node_data) = get_display_rectangle_arena(&[
            (0, RectLayout {
                direction: Some(LayoutDirection::Row),
                .. Default::default()
            }),
            (1, RectLayout {
                max_width: Some(LayoutMaxWidth(PixelValue::px(200.0))),
                padding: Some(LayoutPadding { left: Some(PixelValue::px(20.0)), right: Some(PixelValue::px(20.0)), .. Default::default() }),
                direction: Some(LayoutDirection::Row),
                .. Default::default()
            }),
            (2, RectLayout {
                direction: Some(LayoutDirection::Row),
                .. Default::default()
            })
        ]);

        let preferred_widths = node_data.transform(|_, _| None);
        let mut width_filled_out_data = solve_width::from_rect_layout_arena(&node_data, &preferred_widths);

        // Test some basic stuff - test that `get_flex_basis` works

        // Nodes 0, 2, 3, 4 and 5 have no basis
        assert_eq!(width_filled_out_data[NodeId::new(0)].get_flex_basis_horizontal(), 0.0);

        // Node 1 has a padding on left and right of 20, so a flex-basis of 40.0
        assert_eq!(width_filled_out_data[NodeId::new(1)].get_flex_basis_horizontal(), 40.0);
        assert_eq!(width_filled_out_data[NodeId::new(1)].get_horizontal_padding(), 40.0);

        assert_eq!(width_filled_out_data[NodeId::new(2)].get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out_data[NodeId::new(3)].get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out_data[NodeId::new(4)].get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out_data[NodeId::new(5)].get_flex_basis_horizontal(), 0.0);

        assert_eq!(width_filled_out_data[NodeId::new(0)].preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out_data[NodeId::new(1)].preferred_width, WhConstraint::Between(0.0, 200.0));
        assert_eq!(width_filled_out_data[NodeId::new(2)].preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out_data[NodeId::new(3)].preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out_data[NodeId::new(4)].preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out_data[NodeId::new(5)].preferred_width, WhConstraint::Unconstrained);

        // Test the flex-basis sum
        assert_eq!(solve_width::sum_children_flex_basis(&width_filled_out_data, NodeId::new(2), &node_hierarchy, &node_data), 0.0);
        assert_eq!(solve_width::sum_children_flex_basis(&width_filled_out_data, NodeId::new(1), &node_hierarchy, &node_data), 0.0);
        assert_eq!(solve_width::sum_children_flex_basis(&width_filled_out_data, NodeId::new(0), &node_hierarchy, &node_data), 40.0);

        // -- Section 2: Test that size-bubbling works:
        //
        // Size-bubbling should take the 40px padding and "bubble" it towards the
        let non_leaf_nodes_sorted_by_depth = node_hierarchy.get_parents_sorted_by_depth();

        // ID 5 has no child, so it's not returned, same as 3 and 4
        assert_eq!(non_leaf_nodes_sorted_by_depth, vec![
            (0, NodeId::new(0)),
            (1, NodeId::new(1)),
            (2, NodeId::new(2)),
        ]);

        solve_width::bubble_preferred_widths_to_parents(
            &mut width_filled_out_data,
            &node_hierarchy,
            &node_data,
            &non_leaf_nodes_sorted_by_depth
        );

        // This step shouldn't have touched the flex_grow_px
        for node in &width_filled_out_data.internal {
            assert_eq!(node.flex_grow_px, 0.0);
        }

        // This step should not modify the `preferred_width`
        assert_eq!(width_filled_out_data[NodeId::new(0)].preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out_data[NodeId::new(1)].preferred_width, WhConstraint::Between(0.0, 200.0));
        assert_eq!(width_filled_out_data[NodeId::new(2)].preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out_data[NodeId::new(3)].preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out_data[NodeId::new(4)].preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out_data[NodeId::new(5)].preferred_width, WhConstraint::Unconstrained);

        // The padding of the Node 1 should have bubbled up to be the minimum width of Node 0
        assert_eq!(width_filled_out_data[NodeId::new(0)].min_inner_size_px, 40.0);
        assert_eq!(width_filled_out_data[NodeId::new(1)].get_flex_basis_horizontal(), 40.0);
        assert_eq!(width_filled_out_data[NodeId::new(1)].min_inner_size_px, 0.0);
        assert_eq!(width_filled_out_data[NodeId::new(2)].get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out_data[NodeId::new(2)].min_inner_size_px, 0.0);
        assert_eq!(width_filled_out_data[NodeId::new(3)].get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out_data[NodeId::new(3)].min_inner_size_px, 0.0);
        assert_eq!(width_filled_out_data[NodeId::new(4)].get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out_data[NodeId::new(4)].min_inner_size_px, 0.0);
        assert_eq!(width_filled_out_data[NodeId::new(5)].get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out_data[NodeId::new(5)].min_inner_size_px, 0.0);

        // -- Section 3: Test if growing the sizes works

        let window_width = 754.0; // pixel

        // - window_width: 754px
        // 0                -- [] - expecting width to stretch to 754 px
        // '- 1             -- [max-width: 200px; padding: 20px] - expecting width to stretch to 200 px
        //    '-- 2         -- [] - expecting width to stretch to 160px
        //    '   '-- 3     -- [] - expecting width to stretch to 80px (half of 160)
        //    '   '-- 4     -- [] - expecting width to stretch to 80px (half of 160)
        //    '-- 5         -- [] - expecting width to stretch to 554px (754 - 200px max-width of earlier sibling)

        solve_width::apply_flex_grow(&mut width_filled_out_data, &node_hierarchy, &node_data, &non_leaf_nodes_sorted_by_depth, window_width);

        assert_eq!(width_filled_out_data[NodeId::new(0)].solved_result(), WidthSolvedResult {
            min_width: 40.0,
            space_added: window_width - 40.0,
        });
        assert_eq!(width_filled_out_data[NodeId::new(1)].solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 200.0,
        });
        assert_eq!(width_filled_out_data[NodeId::new(2)].solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 160.0,
        });
        assert_eq!(width_filled_out_data[NodeId::new(3)].solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 80.0,
        });
        assert_eq!(width_filled_out_data[NodeId::new(4)].solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 80.0,
        });
        assert_eq!(width_filled_out_data[NodeId::new(5)].solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: window_width - 200.0,
        });
    }
}
