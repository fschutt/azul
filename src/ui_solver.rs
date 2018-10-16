use std::f32;
use glium::glutin::dpi::LogicalPosition;
use {
    id_tree::{NodeId, Arena},
    css_parser::{LayoutPosition, RectLayout},
    display_list::DisplayRectangle,
    css_parser::{LayoutMargin, LayoutPadding},
};

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

        let mut width = layout.$width.and_then(|w| Some(w.0.to_pixels()));
        let min_width = layout.$min_width.and_then(|w| Some(w.0.to_pixels()));
        let max_width = layout.$max_width.and_then(|w| Some(w.0.to_pixels()));

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

        // We only need to correct the width if the preferred width is in
        // the range between min & max and the width isn't already specified in CSS
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

/// Returns the preferred width, given [width, min_width, max_width] inside a RectLayout
/// or `None` if the height can't be determined from the node alone.
///
// fn determine_preferred_width(layout: &RectLayout) -> Option<f32>
determine_preferred!(determine_preferred_width, width, min_width, max_width);

/// Returns the preferred height, given [height, min_height, max_height] inside a RectLayout
// or `None` if the height can't be determined from the node alone.
///
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
        self.preferred_width.min_needed_space().unwrap_or(0.0) +
        self.margin.left.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.margin.right.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.left.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.right.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0)
    }

    /// Get the sum of the horizontal padding amount (`padding.left + padding.right`)
    pub fn get_horizontal_padding(&self) -> f32 {
        self.padding.left.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.right.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0)
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
        self.margin.top.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.margin.bottom.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.top.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.bottom.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0)
    }

    /// Get the sum of the horizontal padding amount (`padding.top + padding.bottom`)
    pub fn get_vertical_padding(&self) -> f32 {
        self.padding.top.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.bottom.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0)
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
    $struct_name:ident,
    $preferred_field:ident,
    $determine_preferred_fn:ident,
    $get_padding_fn:ident,
    $get_flex_basis:ident,
    $bubble_fn_name:ident,
    $main_axis:ident
) => (

impl Arena<$struct_name> {

    /// Fill out the preferred width of all nodes.
    ///
    /// We could operate on the Arena<DisplayRectangle> directly, but that makes testing very
    /// hard since we are only interested in testing or touching the layout. So this makes the
    /// calculation maybe a few microseconds slower, but gives better testing capabilities
    ///
    /// NOTE: Later on, this could maybe be a Arena<&'a RectLayout>.
    #[must_use]
    fn from_rect_layout_arena(arena: &Arena<RectLayout>, widths: Arena<Option<f32>>) -> Self {
        arena.transform(|node, id| {
            $struct_name {
                // TODO: get the initial width of the rect content
                $preferred_field: $determine_preferred_fn(&node, widths[id].data),
                margin: node.margin.unwrap_or_default(),
                padding: node.padding.unwrap_or_default(),
                flex_grow_px: 0.0,
                min_inner_size_px: 0.0,
            }
        })
    }

    /// Bubble the inner sizes to their parents -  on any parent nodes, fill out
    /// the width so that the `preferred_width` can contain the child nodes (if
    /// that doesn't violate the constraints of the parent)
    #[must_use]
    fn $bubble_fn_name(
        &mut self,
        arena: &Arena<RectLayout>,
        non_leaf_nodes: &[(usize, NodeId)])
    {
        // Reverse, since we want to go from the inside out (depth 5 needs to be filled out first)
        //
        // Set the preferred_width of the parent nodes
        for (_node_depth, non_leaf_id) in non_leaf_nodes.iter().rev() {

            use self::WhConstraint::*;

            // Sum of the direct childrens flex-basis = the parents preferred width
            let children_flex_basis = self.sum_children_flex_basis(*non_leaf_id, arena);

            // Calculate the new flex-basis width
            let parent_width_metrics = self[*non_leaf_id].data;

            // For calculating the inner width, subtract the parents padding
            let parent_padding = self[*non_leaf_id].data.$get_padding_fn();

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

            self[*non_leaf_id].data.min_inner_size_px = child_width;
        }

        // Now, the width of all elements should be filled,
        // but they aren't flex-growed or flex-shrinked yet
    }

    /// Go from the root down and flex_grow the children if needed - respects the `width`, `min_width` and `max_width` properties
    /// The layout step doesn't account for the min_width and max_width constraints, so we have to adjust them manually
    fn apply_flex_grow(
        &mut self,
        arena: &Arena<RectLayout>,
        parent_ids_sorted_by_depth: &[(usize, NodeId)],
        root_width: f32)
    {
        /// Does the actual width layout, respects the `width`, `min_width` and `max_width`
        /// properties as well as the `flex_grow` factor. `flex_shrink` currently does nothing.
        fn distribute_space_along_main_axis(
            node_id: &NodeId,
            arena: &Arena<RectLayout>,
            width_calculated_arena: &mut Arena<$struct_name>)
        {
            // The inner space of the parent node, without the padding
            let mut parent_node_inner_width = {
                let parent_node = &width_calculated_arena[*node_id].data;
                parent_node.min_inner_size_px + parent_node.flex_grow_px - parent_node.$get_padding_fn()
            };

            // 1. Set all child elements that have an exact width to that width, record their violations
            //    and add their violation to the leftover horizontal space.
            // let mut horizontal_space_from_fixed_width_items = 0.0;
            let mut horizontal_space_taken_up_by_fixed_width_items = 0.0;

            {
                // Vec<(NodeId, PreferredWidth)>
                let exact_width_childs = node_id
                        .children(width_calculated_arena)
                        .filter_map(|id| if let WhConstraint::EqualTo(exact) = width_calculated_arena[id].data.$preferred_field {
                            Some((id, exact))
                        } else {
                            None
                        })
                        .collect::<Vec<(NodeId, f32)>>();

                for (exact_width_child_id, preferred_width) in exact_width_childs {
                    // horizontal_space_from_fixed_width_items += violation_px;
                    horizontal_space_taken_up_by_fixed_width_items += preferred_width;
                    // so that node.min_inner_size_px + node.flex_grow_px = preferred_width
                    width_calculated_arena[exact_width_child_id].data.flex_grow_px =
                        preferred_width - width_calculated_arena[exact_width_child_id].data.min_inner_size_px;
                }
            }

            // The fixed-width items are now considered solved, so subtract them out of the width of the parent.
            parent_node_inner_width -= horizontal_space_taken_up_by_fixed_width_items;

            // Now we can be sure that if we write #x { width: 500px; } that it will actually be 500px large
            // and not be influenced by flex in any way.

            // 2. Set all items to their minimium width. Record how much space is gained by doing so.
            let mut horizontal_space_taken_up_by_variable_items = 0.0;

            use FastHashSet;

            let mut variable_width_childs = node_id
                .children(width_calculated_arena)
                .filter(|id| !width_calculated_arena[*id].data.$preferred_field.is_fixed_constraint())
                .collect::<FastHashSet<NodeId>>();

            for variable_child_id in &variable_width_childs {
                let min_width = width_calculated_arena[*variable_child_id].data.$preferred_field.min_needed_space().unwrap_or(0.0);
                horizontal_space_taken_up_by_variable_items += min_width;

                // so that node.min_inner_size_px + node.flex_grow_px = min_width
                width_calculated_arena[*variable_child_id].data.flex_grow_px =
                    min_width - width_calculated_arena[*variable_child_id].data.min_inner_size_px;
            }

            // This satisfies the `width` and `min_width` constraints. However, we still need to worry about
            // the `max_width` and unconstrained childs
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
                            arena[*child_id].data.flex_grow
                                .and_then(|grow| Some(grow.0.max(1.0)))
                                .unwrap_or(DEFAULT_FLEX_GROW_FACTOR))
                    .sum();

                // Grow all variable children by the same amount.
                for variable_child_id in &variable_width_childs {

                    let flex_grow = arena[*variable_child_id].data.flex_grow
                        .and_then(|grow| Some(grow.0.max(1.0)))
                        .unwrap_or(DEFAULT_FLEX_GROW_FACTOR);

                    let added_space_for_one_child = total_horizontal_space_available * (flex_grow / children_combined_flex_grow);

                    let current_width_of_child = width_calculated_arena[*variable_child_id].data.min_inner_size_px +
                                                 width_calculated_arena[*variable_child_id].data.flex_grow_px;

                    if let Some(max_width) = width_calculated_arena[*variable_child_id].data.$preferred_field.max_available_space() {
                        if (current_width_of_child + added_space_for_one_child) > max_width {
                            // so that node.min_inner_size_px + node.flex_grow_px = max_width
                            width_calculated_arena[*variable_child_id].data.flex_grow_px =
                                max_width - width_calculated_arena[*variable_child_id].data.min_inner_size_px;

                            max_width_violations.push(*variable_child_id);
                        } else {
                            // so that node.min_inner_size_px + node.flex_grow_px = added_space_for_one_child
                            width_calculated_arena[*variable_child_id].data.flex_grow_px =
                                added_space_for_one_child - width_calculated_arena[*variable_child_id].data.min_inner_size_px;
                        }
                    } else {
                        // so that node.min_inner_size_px + node.flex_grow_px = added_space_for_one_child
                        width_calculated_arena[*variable_child_id].data.flex_grow_px =
                            added_space_for_one_child - width_calculated_arena[*variable_child_id].data.min_inner_size_px;
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
                            width_calculated_arena[solved_node_id].data.min_inner_size_px +
                            width_calculated_arena[solved_node_id].data.flex_grow_px;

                        variable_width_childs.remove(&solved_node_id);
                    }
                }
            }
        }

        fn distribute_space_along_cross_axis(
            node_id: &NodeId,
            arena: &Arena<RectLayout>,
            width_calculated_arena: &mut Arena<$struct_name>)
        {
            // The inner space of the parent node, without the padding
            let parent_node_inner_width = {
                let parent_node = &width_calculated_arena[*node_id].data;
                parent_node.min_inner_size_px + parent_node.flex_grow_px - parent_node.$get_padding_fn()
            };

            for child_id in node_id.children(arena) {

                let preferred_width = {
                    let min_width = width_calculated_arena[child_id].data.$preferred_field.min_needed_space().unwrap_or(0.0);
                    // In this case we want to overflow if the min width of the cross axis
                    if min_width > parent_node_inner_width {
                        min_width
                    } else {
                        if let Some(max_width) = width_calculated_arena[child_id].data.$preferred_field.max_available_space() {
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
                width_calculated_arena[child_id].data.flex_grow_px =
                    preferred_width - width_calculated_arena[child_id].data.min_inner_size_px;
            }
        }

        debug_assert!(self[NodeId::new(0)].data.flex_grow_px == 0.0);

        // Set the window width on the root node (since there is only one root node, we can
        // calculate the `flex_grow_px` directly)
        //
        // Usually `top_level_flex_basis` is NOT 0.0, rather it's the sum of all widths in the DOM,
        // i.e. the sum of the whole DOM tree
        let top_level_flex_basis = self[NodeId::new(0)].data.min_inner_size_px;
        self[NodeId::new(0)].data.flex_grow_px = root_width - top_level_flex_basis;

        for (_node_depth, parent_id) in parent_ids_sorted_by_depth {
            use css_parser::LayoutAxis;

            if arena[*parent_id].data.direction.unwrap_or_default().get_axis() == LayoutAxis::$main_axis {
                distribute_space_along_main_axis(parent_id, arena, self);
            } else {
                distribute_space_along_cross_axis(parent_id, arena, self);
            }
        }
    }

    /// Returns the sum of the flex-basis of the current nodes' children
    fn sum_children_flex_basis(
        &self,
        node_id: NodeId,
        display_arena: &Arena<RectLayout>)
    -> f32
    {
        node_id
            .children(self)
            .filter(|child_node_id| display_arena[*child_node_id].data.position != Some(LayoutPosition::Absolute))
            .map(|child_node_id| self[child_node_id].data.$get_flex_basis())
            .sum()
    }
}

)}

typed_arena!(
    WidthCalculatedRect,
    preferred_width,
    determine_preferred_width,
    get_horizontal_padding,
    get_flex_basis_horizontal,
    bubble_preferred_widths_to_parents,
    Horizontal
);

typed_arena!(
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
    pub solved_widths: Arena<WidthSolvedResult>,
    pub layout_only_arena: Arena<RectLayout>,
    pub non_leaf_nodes_sorted_by_depth: Vec<(usize, NodeId)>,
}

#[derive(Debug, Clone)]
pub(crate) struct SolvedHeightLayout {
    pub solved_heights: Arena<HeightSolvedResult>,
}

/// Returns the solved widths of the items in a BTree form
pub(crate) fn solve_flex_layout_width<'a>(
    display_rectangles: &Arena<DisplayRectangle<'a>>,
    preferred_widths: Arena<Option<f32>>,
    window_width: f32)
-> SolvedWidthLayout
{
    let layout_only_arena = display_rectangles.transform(|node, _| node.layout);
    let mut width_calculated_arena = Arena::<WidthCalculatedRect>::from_rect_layout_arena(&layout_only_arena, preferred_widths);
    let non_leaf_nodes_sorted_by_depth = get_non_leaf_nodes_sorted_by_depth(&layout_only_arena);
    width_calculated_arena.bubble_preferred_widths_to_parents(&layout_only_arena, &non_leaf_nodes_sorted_by_depth);
    width_calculated_arena.apply_flex_grow(&layout_only_arena, &non_leaf_nodes_sorted_by_depth, window_width);
    let solved_widths = width_calculated_arena.transform(|node, _| node.solved_result());
    SolvedWidthLayout { solved_widths , layout_only_arena, non_leaf_nodes_sorted_by_depth }
}

/// Returns the solved height of the items in a BTree form
pub(crate) fn solve_flex_layout_height(
    solved_widths: &SolvedWidthLayout,
    preferred_heights: Arena<Option<f32>>,
    window_height: f32)
-> SolvedHeightLayout
{
    let SolvedWidthLayout { layout_only_arena, .. } = solved_widths;
    let mut height_calculated_arena = Arena::<HeightCalculatedRect>::from_rect_layout_arena(&layout_only_arena, preferred_heights);
    height_calculated_arena.bubble_preferred_heights_to_parents(&layout_only_arena, &solved_widths.non_leaf_nodes_sorted_by_depth);
    height_calculated_arena.apply_flex_grow(&layout_only_arena, &solved_widths.non_leaf_nodes_sorted_by_depth, window_height);
    let solved_heights = height_calculated_arena.transform(|node, _| node.solved_result());
    SolvedHeightLayout { solved_heights }
}

/// Traverses from arena[id] to the root, returning the amount of parents, i.e. the depth of the node in the tree.
#[inline]
fn leaf_node_depth<T>(id: &NodeId, arena: &Arena<T>) -> usize {
    let mut counter = 0;
    let mut last_id = *id;

    while let Some(parent) = arena[last_id].parent {
        last_id = parent;
        counter += 1;
    }

    counter
}

/// Returns the nearest common ancestor with a `position: relative` attribute
/// or `None` if there is no ancestor that has `position: relative`. Usually
/// used in conjunction with `position: absolute`
fn get_nearest_positioned_ancestor<'a>(start_node_id: NodeId, arena: &Arena<RectLayout>)
-> Option<NodeId>
{
    let mut current_node = start_node_id;
    while let Some(parent) = arena[current_node].parent() {
        // An element with position: absolute; is positioned relative to the nearest
        // positioned ancestor (instead of positioned relative to the viewport, like fixed).
        //
        // A "positioned" element is one whose position is anything except static.
        if let Some(LayoutPosition::Static) = arena[parent].data.position {
            current_node = parent;
        } else {
            return Some(parent);
        }
    }
    None
}

/// Returns the `(depth, NodeId)` of all non-leaf nodes (i.e. nodes that have a
/// `first_child`), in depth sorted order, (i.e. `NodeId(0)` with a depth of 0) is
/// the first element.
///
/// Runtime: O(n) max
fn get_non_leaf_nodes_sorted_by_depth<T>(arena: &Arena<T>) -> Vec<(usize, NodeId)> {

    let mut non_leaf_nodes = Vec::new();
    let mut current_children = vec![(0, NodeId::new(0))];
    let mut next_children = Vec::new();
    let mut depth = 1;

    loop {

        for id in &current_children {
            for child_id in id.1.children(arena).filter(|id| arena[*id].first_child.is_some()) {
                next_children.push((depth, child_id));
            }
        }

        non_leaf_nodes.extend(&mut current_children.drain(..));

        if next_children.is_empty() {
            break;
        } else {
            current_children.extend(&mut next_children.drain(..));
            depth += 1;
        }
    }

    non_leaf_nodes
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub(crate) struct HorizontalSolvedPosition(pub f32);

/// Traverses along the DOM and solved
pub(crate) fn get_x_positions(solved_widths: &SolvedWidthLayout, origin: LogicalPosition)
-> Arena<HorizontalSolvedPosition>
{
    use css_parser::LayoutAxis;

    let arena = &solved_widths.layout_only_arena;
    let non_leaf_nodes = &solved_widths.non_leaf_nodes_sorted_by_depth;
    let widths = &solved_widths.solved_widths;

    let mut arena_solved = widths.transform(|_, _| HorizontalSolvedPosition(0.0));

    arena_solved[NodeId::new(0)].data = HorizontalSolvedPosition(origin.x as f32);

    for (_node_depth, parent_id) in non_leaf_nodes {

        let parent_padding = arena[*parent_id].data.padding.unwrap_or_default();
        let parent_padding_left = parent_padding.left.and_then(|x| Some(x.to_pixels())).unwrap_or(0.0);
        let parent_padding_right = parent_padding.right.and_then(|x| Some(x.to_pixels())).unwrap_or(0.0);
        let parent_x_position = arena_solved[*parent_id].data.0 + parent_padding_left;

        if arena[*parent_id].data.direction.unwrap_or_default().get_axis() == LayoutAxis::Vertical {
            // Along main axis: Take X of parent
            for child_id in parent_id.children(arena) {
                arena_solved[child_id].data.0 = parent_x_position;
            }
        } else {
            // Along cross axis: Increase X with width of current element

            let parent_inner_width = {
                let parent_node = &widths[*parent_id].data;
                parent_node.min_width + parent_node.space_added - (parent_padding_left + parent_padding_right)
            };

            let main_axis_alignment = arena[*parent_id].data.justify_content.unwrap_or_default();

            let mut sum_x_of_children_so_far = 0.0;

            for child_id in parent_id.children(arena) {

                use css_parser::LayoutJustifyContent::*;

                // width: increase X according to the main axis, Y according to the cross_axis
                let child_margin = arena[child_id].data.margin.unwrap_or_default();
                let child_margin_left = child_margin.left.and_then(|x| Some(x.to_pixels())).unwrap_or(0.0);
                let child_margin_right = child_margin.right.and_then(|x| Some(x.to_pixels())).unwrap_or(0.0);

                let child_width_with_padding = {
                    let child_node = &widths[child_id].data;
                    child_node.min_width + child_node.space_added
                };

                // Always the top left corner
                let x_of_top_left_corner = match main_axis_alignment {
                    Start => {
                        parent_x_position + sum_x_of_children_so_far + child_margin_left
                    },
                    End => {
                        (parent_x_position + parent_inner_width) - (sum_x_of_children_so_far + child_margin_right + child_width_with_padding)
                    },
                    Center => {
                        parent_x_position + ((parent_inner_width / 2.0) - ((sum_x_of_children_so_far + child_margin_right + child_width_with_padding) / 2.0))
                    },
                    SpaceBetween => {
                        parent_x_position // TODO!
                    },
                    SpaceAround => {
                        parent_x_position // TODO!
                    },
                };

                arena_solved[child_id].data.0 = x_of_top_left_corner;

                sum_x_of_children_so_far += child_margin_right + child_width_with_padding + child_margin_left;
            }
        }
    }

    arena_solved
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub(crate) struct VerticalSolvedPosition(pub f32);

/// Traverses along the DOM and solved
pub(crate) fn get_y_positions(solved_heights: &SolvedHeightLayout, width_arena: &SolvedWidthLayout, origin: LogicalPosition)
-> Arena<VerticalSolvedPosition>
{
    use css_parser::LayoutAxis;

    let arena = &width_arena.layout_only_arena;
    let non_leaf_nodes = &width_arena.non_leaf_nodes_sorted_by_depth;
    let heights = &solved_heights.solved_heights;

    let mut arena_solved = heights.transform(|_, _| VerticalSolvedPosition(0.0));

    arena_solved[NodeId::new(0)].data = VerticalSolvedPosition(origin.y as f32);

    for (_node_depth, parent_id) in non_leaf_nodes {

        let parent_padding = arena[*parent_id].data.padding.unwrap_or_default();
        let parent_padding_top = parent_padding.top.and_then(|x| Some(x.to_pixels())).unwrap_or(0.0);
        let parent_padding_bottom = parent_padding.bottom.and_then(|x| Some(x.to_pixels())).unwrap_or(0.0);
        let parent_y_position = arena_solved[*parent_id].data.0 + parent_padding_top;

        if arena[*parent_id].data.direction.unwrap_or_default().get_axis() == LayoutAxis::Horizontal {
            // Along main axis: Take Y of parent
            for child_id in parent_id.children(arena) {
                arena_solved[child_id].data.0 = parent_y_position;
            }
        } else {
            // Cross axis: increase Y
            let parent_inner_height = {
                let parent_node = &heights[*parent_id].data;
                parent_node.min_height + parent_node.space_added - (parent_padding_top + parent_padding_bottom)
            };

            let main_axis_alignment = arena[*parent_id].data.justify_content.unwrap_or_default();

            let mut sum_y_of_children_so_far = 0.0;

            for child_id in parent_id.children(arena) {

                use css_parser::LayoutJustifyContent::*;

                // width: increase X according to the main axis, Y according to the cross_axis
                let child_margin = arena[child_id].data.margin.unwrap_or_default();
                let child_margin_top = child_margin.top.and_then(|x| Some(x.to_pixels())).unwrap_or(0.0);
                let child_margin_bottom = child_margin.bottom.and_then(|x| Some(x.to_pixels())).unwrap_or(0.0);

                let child_height_with_padding = {
                    let child_node = &heights[child_id].data;
                    child_node.min_height + child_node.space_added
                };

                // Always the top left corner
                let y_of_top_left_corner = match main_axis_alignment {
                    Start => {
                        parent_y_position + sum_y_of_children_so_far + child_margin_top
                    },
                    End => {
                        (parent_y_position + parent_inner_height) - (sum_y_of_children_so_far + child_margin_top + child_height_with_padding)
                    },
                    Center => {
                        parent_y_position + ((parent_inner_height / 2.0) - ((sum_y_of_children_so_far + child_margin_top + child_height_with_padding) / 2.0))
                    },
                    SpaceBetween => {
                        parent_y_position // TODO!
                    },
                    SpaceAround => {
                        parent_y_position // TODO!
                    },
                };

                arena_solved[child_id].data.0 = y_of_top_left_corner;

                sum_y_of_children_so_far += child_margin_top + child_height_with_padding + child_margin_bottom;
            }
        }
    }

    arena_solved
}

#[cfg(test)]
mod layout_tests {

    use css_parser::RectLayout;
    use id_tree::{Arena, Node, NodeId};
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
    fn get_testing_dom() -> Arena<()> {
        Arena {
            nodes: vec![
                // 0
                Node {
                    parent: None,
                    previous_sibling: None,
                    next_sibling: None,
                    first_child: Some(NodeId::new(1)),
                    last_child: Some(NodeId::new(1)),
                    data: (),
                },
                // 1
                Node {
                    parent: Some(NodeId::new(0)),
                    previous_sibling: None,
                    next_sibling: Some(NodeId::new(5)),
                    first_child: Some(NodeId::new(2)),
                    last_child: Some(NodeId::new(2)),
                    data: (),
                },
                // 2
                Node {
                    parent: Some(NodeId::new(1)),
                    previous_sibling: None,
                    next_sibling: None,
                    first_child: Some(NodeId::new(3)),
                    last_child: Some(NodeId::new(4)),
                    data: (),
                },
                // 3
                Node {
                    parent: Some(NodeId::new(2)),
                    previous_sibling: None,
                    next_sibling: Some(NodeId::new(4)),
                    first_child: None,
                    last_child: None,
                    data: (),
                },
                // 4
                Node {
                    parent: Some(NodeId::new(2)),
                    previous_sibling: Some(NodeId::new(3)),
                    next_sibling: None,
                    first_child: None,
                    last_child: None,
                    data: (),
                },
                // 5
                Node {
                    parent: Some(NodeId::new(1)),
                    previous_sibling: Some(NodeId::new(2)),
                    next_sibling: None,
                    first_child: None,
                    last_child: None,
                    data: (),
                },
            ]
        }
    }

    /// Returns the same arena, but pre-fills nodes at [(NodeId, RectLayout)]
    /// with the layout rect
    fn get_display_rectangle_arena(constraints: &[(usize, RectLayout)]) -> Arena<RectLayout> {
        let arena = get_testing_dom();
        let mut arena = arena.transform(|_, _| RectLayout::default());
        for (id, rect) in constraints {
            arena[NodeId::new(*id)].data = *rect;
        }
        arena
    }

    #[test]
    fn test_determine_preferred_width() {
        use css_parser::{LayoutMinWidth, LayoutMaxWidth, PixelValue, LayoutWidth};

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

        use css_parser::*;

        let display_rectangles = get_display_rectangle_arena(&[
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

        let preferred_widths = display_rectangles.transform(|_, _| None);
        let mut width_filled_out = Arena::<WidthCalculatedRect>::from_rect_layout_arena(&display_rectangles, preferred_widths);

        // Test some basic stuff - test that `get_flex_basis` works

        // Nodes 0, 2, 3, 4 and 5 have no basis
        assert_eq!(width_filled_out[NodeId::new(0)].data.get_flex_basis_horizontal(), 0.0);

        // Node 1 has a padding on left and right of 20, so a flex-basis of 40.0
        assert_eq!(width_filled_out[NodeId::new(1)].data.get_flex_basis_horizontal(), 40.0);
        assert_eq!(width_filled_out[NodeId::new(1)].data.get_horizontal_padding(), 40.0);

        assert_eq!(width_filled_out[NodeId::new(2)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(3)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(4)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(5)].data.get_flex_basis_horizontal(), 0.0);

        assert_eq!(width_filled_out[NodeId::new(0)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(1)].data.preferred_width, WhConstraint::Between(0.0, 200.0));
        assert_eq!(width_filled_out[NodeId::new(2)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(3)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(4)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(5)].data.preferred_width, WhConstraint::Unconstrained);

        // Test the flex-basis sum
        assert_eq!(width_filled_out.sum_children_flex_basis(NodeId::new(2), &display_rectangles), 0.0);
        assert_eq!(width_filled_out.sum_children_flex_basis(NodeId::new(1), &display_rectangles), 0.0);
        assert_eq!(width_filled_out.sum_children_flex_basis(NodeId::new(0), &display_rectangles), 40.0);

        // -- Section 2: Test that size-bubbling works:
        //
        // Size-bubbling should take the 40px padding and "bubble" it towards the
        let non_leaf_nodes_sorted_by_depth = get_non_leaf_nodes_sorted_by_depth(&display_rectangles);

        // ID 5 has no child, so it's not returned, same as 3 and 4
        assert_eq!(non_leaf_nodes_sorted_by_depth, vec![
            (0, NodeId::new(0)),
            (1, NodeId::new(1)),
            (2, NodeId::new(2)),
        ]);

        width_filled_out.bubble_preferred_widths_to_parents(&display_rectangles, &non_leaf_nodes_sorted_by_depth);


        // This step shouldn't have touched the flex_grow_px
        for node_id in width_filled_out.linear_iter() {
            assert_eq!(width_filled_out[node_id].data.flex_grow_px, 0.0);
        }

        // This step should not modify the `preferred_width`
        assert_eq!(width_filled_out[NodeId::new(0)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(1)].data.preferred_width, WhConstraint::Between(0.0, 200.0));
        assert_eq!(width_filled_out[NodeId::new(2)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(3)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(4)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(5)].data.preferred_width, WhConstraint::Unconstrained);

        // The padding of the Node 1 should have bubbled up to be the minimum width of Node 0
        assert_eq!(width_filled_out[NodeId::new(0)].data.min_inner_size_px, 40.0);
        assert_eq!(width_filled_out[NodeId::new(1)].data.get_flex_basis_horizontal(), 40.0);
        assert_eq!(width_filled_out[NodeId::new(1)].data.min_inner_size_px, 0.0);
        assert_eq!(width_filled_out[NodeId::new(2)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(2)].data.min_inner_size_px, 0.0);
        assert_eq!(width_filled_out[NodeId::new(3)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(3)].data.min_inner_size_px, 0.0);
        assert_eq!(width_filled_out[NodeId::new(4)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(4)].data.min_inner_size_px, 0.0);
        assert_eq!(width_filled_out[NodeId::new(5)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(5)].data.min_inner_size_px, 0.0);

        // -- Section 3: Test if growing the sizes works

        let window_width = 754.0; // pixel

        // - window_width: 754px
        // 0                -- [] - expecting width to stretch to 754 px
        // '- 1             -- [max-width: 200px; padding: 20px] - expecting width to stretch to 200 px
        //    '-- 2         -- [] - expecting width to stretch to 160px
        //    '   '-- 3     -- [] - expecting width to stretch to 80px (half of 160)
        //    '   '-- 4     -- [] - expecting width to stretch to 80px (half of 160)
        //    '-- 5         -- [] - expecting width to stretch to 554px (754 - 200px max-width of earlier sibling)

        width_filled_out.apply_flex_grow(&display_rectangles, &non_leaf_nodes_sorted_by_depth, window_width);

        assert_eq!(width_filled_out[NodeId::new(0)].data.solved_result(), WidthSolvedResult {
            min_width: 40.0,
            space_added: window_width - 40.0,
        });
        assert_eq!(width_filled_out[NodeId::new(1)].data.solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 200.0,
        });
        assert_eq!(width_filled_out[NodeId::new(2)].data.solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 160.0,
        });
        assert_eq!(width_filled_out[NodeId::new(3)].data.solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 80.0,
        });
        assert_eq!(width_filled_out[NodeId::new(4)].data.solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 80.0,
        });
        assert_eq!(width_filled_out[NodeId::new(5)].data.solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: window_width - 200.0,
        });
    }

    /// Tests that the node-depth calculation works correctly
    #[test]
    fn test_leaf_node_depth() {

        let arena = get_testing_dom();

        // 0                -- depth 0
        // '- 1             -- depth 1
        //    '-- 2         -- depth 2
        //    '   '-- 3     -- depth 3
        //    '   '--- 4    -- depth 3
        //    '-- 5         -- depth 2

        assert_eq!(leaf_node_depth(&NodeId::new(0), &arena), 0);
        assert_eq!(leaf_node_depth(&NodeId::new(1), &arena), 1);
        assert_eq!(leaf_node_depth(&NodeId::new(2), &arena), 2);
        assert_eq!(leaf_node_depth(&NodeId::new(3), &arena), 3);
        assert_eq!(leaf_node_depth(&NodeId::new(4), &arena), 3);
        assert_eq!(leaf_node_depth(&NodeId::new(5), &arena), 2);
    }
}