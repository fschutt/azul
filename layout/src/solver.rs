use alloc::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    string::ToString,
    vec::Vec,
};
use core::f32;

use azul_core::{
    app_resources::{
        DpiScaleFactor, Epoch, FontInstanceKey, IdNamespace, ImageCache, RendererResources,
        ResourceUpdate, ShapedWords, WordPositions, Words,
    },
    callbacks::DocumentId,
    display_list::RenderCallbacks,
    dom::{NodeData, NodeType},
    id_tree::{NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut, NodeId},
    styled_dom::{
        ChangedCssProperty, CssPropertyCache, DomId, NodeHierarchyItem, ParentWithNodeDepth,
        StyledDom, StyledNode, StyledNodeState,
    },
    traits::GetTextLayout,
    ui_solver::{
        GpuValueCache, HeightCalculatedRect, HorizontalSolvedPosition, LayoutDebugMessage,
        LayoutResult, PositionInfoInner, PositionedRectangle, RelayoutChanges, ResolvedOffsets,
        ScrolledNodes, StyleBoxShadowOffsets, VerticalSolvedPosition, WhConstraint,
        WidthCalculatedRect, DEFAULT_FONT_SIZE_PX, DEFAULT_WORD_SPACING,
    },
    window::{FullWindowState, LogicalPosition, LogicalRect, LogicalSize},
};
#[cfg(feature = "text_layout")]
use azul_core::{
    callbacks::{CallbackInfo, DomNodeId, InlineText},
    ui_solver::ResolvedTextLayoutOptions,
};
use azul_css::*;
use rust_fontconfig::FcFontCache;

use crate::text;

const DEFAULT_FLEX_GROW_FACTOR: f32 = 0.0;

#[derive(Debug)]
pub struct WhConfig {
    pub width: WidthConfig,
    pub height: HeightConfig,
}

#[derive(Debug, Default)]
pub struct WidthConfig {
    pub exact: Option<LayoutWidth>,
    pub max: Option<LayoutMaxWidth>,
    pub min: Option<LayoutMinWidth>,
    pub overflow: Option<LayoutOverflow>,
}

#[derive(Debug, Default)]
pub struct HeightConfig {
    pub exact: Option<LayoutHeight>,
    pub max: Option<LayoutMaxHeight>,
    pub min: Option<LayoutMinHeight>,
    pub overflow: Option<LayoutOverflow>,
}

fn precalculate_wh_config(styled_dom: &StyledDom) -> NodeDataContainer<WhConfig> {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();

    NodeDataContainer {
        internal: styled_dom
            .styled_nodes
            .as_container()
            .internal
            .iter()
            .enumerate()
            .map(|(node_id, styled_node)| {
                let node_id = NodeId::new(node_id);
                WhConfig {
                    width: WidthConfig {
                        exact: css_property_cache
                            .get_width(&node_data_container[node_id], &node_id, &styled_node.state)
                            .and_then(|p| p.get_property().copied()),
                        max: css_property_cache
                            .get_max_width(
                                &node_data_container[node_id],
                                &node_id,
                                &styled_node.state,
                            )
                            .and_then(|p| p.get_property().copied()),
                        min: css_property_cache
                            .get_min_width(
                                &node_data_container[node_id],
                                &node_id,
                                &styled_node.state,
                            )
                            .and_then(|p| p.get_property().copied()),
                        overflow: css_property_cache
                            .get_overflow_x(
                                &node_data_container[node_id],
                                &node_id,
                                &styled_node.state,
                            )
                            .and_then(|p| p.get_property().copied()),
                    },
                    height: HeightConfig {
                        exact: css_property_cache
                            .get_height(&node_data_container[node_id], &node_id, &styled_node.state)
                            .and_then(|p| p.get_property().copied()),
                        max: css_property_cache
                            .get_max_height(
                                &node_data_container[node_id],
                                &node_id,
                                &styled_node.state,
                            )
                            .and_then(|p| p.get_property().copied()),
                        min: css_property_cache
                            .get_min_height(
                                &node_data_container[node_id],
                                &node_id,
                                &styled_node.state,
                            )
                            .and_then(|p| p.get_property().copied()),
                        overflow: css_property_cache
                            .get_overflow_y(
                                &node_data_container[node_id],
                                &node_id,
                                &styled_node.state,
                            )
                            .and_then(|p| p.get_property().copied()),
                    },
                }
            })
            .collect(),
    }
}

macro_rules! determine_preferred {
    ($fn_name:ident, $width:ident) => {
        /// - `preferred_inner_width` denotes the preferred width of the
        /// width or height got from the from the rectangles content.
        ///
        /// For example, if you have an image, the `preferred_inner_width` is the images width,
        /// if the node type is an text, the `preferred_inner_width` is the text height.
        fn $fn_name(
            config: &WhConfig,
            preferred_width: Option<f32>,
            parent_width: f32,
            parent_overflow: LayoutOverflow,
        ) -> WhConstraint {
            let width = config
                .$width
                .exact
                .as_ref()
                .map(|x| x.inner.to_pixels(parent_width).max(0.0));
            let min_width = config
                .$width
                .min
                .as_ref()
                .map(|x| x.inner.to_pixels(parent_width).max(0.0));
            let max_width = config
                .$width
                .max
                .as_ref()
                .map(|x| x.inner.to_pixels(parent_width).max(0.0));

            if let Some(width) = width {
                // ignore preferred_width if the width is set manually
                WhConstraint::EqualTo(
                    width
                        .min(max_width.unwrap_or(f32::MAX))
                        .max(min_width.unwrap_or(0.0)),
                )
            } else {
                // no width, only min_width and max_width
                if let Some(max_width) = max_width {
                    WhConstraint::Between(
                        min_width
                            .unwrap_or(0.0)
                            .max(preferred_width.unwrap_or(0.0))
                            .min(max_width.min(f32::MAX)),
                        max_width.max(0.0),
                    )
                } else {
                    // no width or max_width, only min_width
                    if let Some(min_width) = min_width {
                        if min_width.max(preferred_width.unwrap_or(0.0)) < parent_width.max(0.0) {
                            WhConstraint::Between(
                                min_width.max(preferred_width.unwrap_or(0.0)),
                                parent_width.max(0.0),
                            )
                        } else {
                            WhConstraint::EqualTo(min_width.max(preferred_width.unwrap_or(0.0)))
                        }
                    } else {
                        // no width, min_width or max_width: try preferred width
                        if let Some(preferred_width) = preferred_width {
                            let preferred_max = preferred_width.max(0.0);
                            if preferred_max > parent_width {
                                match parent_overflow {
                                    LayoutOverflow::Hidden | LayoutOverflow::Visible => {
                                        WhConstraint::Between(preferred_max, core::f32::MAX)
                                    }
                                    LayoutOverflow::Auto | LayoutOverflow::Scroll => {
                                        WhConstraint::EqualTo(parent_width)
                                    }
                                }
                            } else {
                                WhConstraint::Between(preferred_max, parent_width)
                            }
                        } else {
                            match parent_overflow {
                                LayoutOverflow::Hidden | LayoutOverflow::Visible => {
                                    WhConstraint::Between(0.0, core::f32::MAX)
                                }
                                LayoutOverflow::Auto | LayoutOverflow::Scroll => {
                                    WhConstraint::Between(0.0, parent_width)
                                }
                            }
                        }
                    }
                }
            }
        }
    };
}

// Returns the preferred width, given [width, min_width, max_width] inside a RectLayout
// or `None` if the height can't be determined from the node alone.
//
// fn determine_preferred_width(layout: &RectLayout) -> Option<f32>
determine_preferred!(determine_preferred_width, width);

// Returns the preferred height, given [height, min_height, max_height] inside a RectLayout
// or `None` if the height can't be determined from the node alone.
//
// fn determine_preferred_height(layout: &RectLayout) -> Option<f32>
determine_preferred!(determine_preferred_height, height);

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
macro_rules! typed_arena {
    (
        $struct_name:ident,
        $preferred_field:ident,
        $determine_preferred_fn:ident,
        $width_or_height:ident,
        $get_padding_fn:ident,
        $get_border_fn:ident,
        $get_margin_fn:ident,
        $get_flex_basis:ident,
        $from_rect_layout_arena_fn_name:ident,
        $bubble_fn_name:ident,
        $apply_flex_grow_fn_name:ident,
        $main_axis:ident,
        $margin_left:ident,
        $margin_right:ident,
        $padding_left:ident,
        $padding_right:ident,
        $border_left:ident,
        $border_right:ident,
        $left:ident,
        $right:ident,
    ) => {
        /// Fill out the preferred width of all nodes.
        ///
        /// We could operate on the NodeDataContainer<StyledNode> directly,
        /// but that makes testing very hard since we are only interested
        /// in testing or touching the layout. So this makes the
        /// calculation maybe a few microseconds slower, but gives better
        /// testing capabilities
        ///
        /// NOTE: Later on, this could maybe be a NodeDataContainer<&'a RectLayout>.
        #[must_use]
        fn $from_rect_layout_arena_fn_name<'a>(
            wh_configs: &NodeDataContainerRef<'a, WhConfig>,
            offsets: &NodeDataContainerRef<'a, AllOffsets>,
            widths: &NodeDataContainerRef<'a, Option<f32>>,
            node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
            node_depths: &[ParentWithNodeDepth],
            root_size_width: f32,
        ) -> NodeDataContainer<$struct_name> {
            // then calculate the widths again, but this time using the parent nodes
            let mut new_nodes = NodeDataContainer {
                internal: vec![$struct_name::default(); node_hierarchy.len()],
            };

            for ParentWithNodeDepth { depth: _, node_id } in node_depths.iter() {
                let parent_id = match node_id.into_crate_internal() {
                    Some(s) => s,
                    None => continue,
                };

                let nd = &wh_configs[parent_id];
                let parent_offsets = &offsets[parent_id];
                let width = match widths.get(parent_id) {
                    Some(s) => *s,
                    None => continue,
                };

                let parent_parent_id = node_hierarchy
                    .get(parent_id)
                    .and_then(|t| t.parent_id())
                    .unwrap_or(NodeId::ZERO);

                let parent_parent_width = new_nodes
                    .as_ref()
                    .get(parent_parent_id)
                    .map(|parent| parent.$preferred_field)
                    .unwrap_or_default()
                    .max_available_space()
                    .unwrap_or(root_size_width);

                let parent_parent_overflow = wh_configs[parent_parent_id]
                    .$width_or_height
                    .overflow
                    .unwrap_or_default();

                let parent_width = $determine_preferred_fn(
                    &nd,
                    width,
                    parent_parent_width,
                    parent_parent_overflow,
                );

                new_nodes.as_ref_mut()[parent_id] = $struct_name {
                    // TODO: get the initial width of the rect content
                    $preferred_field: parent_width,

                    $margin_left: parent_offsets.margin.$left.as_ref().copied(),
                    $margin_right: parent_offsets.margin.$right.as_ref().copied(),

                    $padding_left: parent_offsets.padding.$left.as_ref().copied(),
                    $padding_right: parent_offsets.padding.$right.as_ref().copied(),

                    $border_left: parent_offsets.border_widths.$left.as_ref().copied(),
                    $border_right: parent_offsets.border_widths.$right.as_ref().copied(),

                    $left: parent_offsets.position.$left.as_ref().copied(),
                    $right: parent_offsets.position.$right.as_ref().copied(),

                    box_sizing: parent_offsets.box_sizing,
                    flex_grow_px: 0.0,
                    min_inner_size_px: parent_width.min_needed_space().unwrap_or(0.0),
                };

                let parent_overflow = wh_configs[parent_id]
                    .$width_or_height
                    .overflow
                    .unwrap_or_default();

                for child_id in parent_id.az_children(node_hierarchy) {
                    let nd = &wh_configs[child_id];
                    let child_offsets = &offsets[child_id];
                    let width = match widths.get(child_id) {
                        Some(s) => *s,
                        None => continue,
                    };
                    let parent_available_space = parent_width.max_available_space().unwrap_or(0.0);
                    let child_width = $determine_preferred_fn(
                        &nd,
                        width,
                        parent_available_space,
                        parent_overflow,
                    );
                    let mut child = $struct_name {
                        // TODO: get the initial width of the rect content
                        $preferred_field: child_width,

                        $margin_left: child_offsets.margin.$left.as_ref().copied(),
                        $margin_right: child_offsets.margin.$right.as_ref().copied(),

                        $padding_left: child_offsets.padding.$left.as_ref().copied(),
                        $padding_right: child_offsets.padding.$right.as_ref().copied(),

                        $border_left: child_offsets.border_widths.$left.as_ref().copied(),
                        $border_right: child_offsets.border_widths.$right.as_ref().copied(),

                        $left: child_offsets.position.$left.as_ref().copied(),
                        $right: child_offsets.position.$right.as_ref().copied(),

                        box_sizing: child_offsets.box_sizing,
                        flex_grow_px: 0.0,
                        min_inner_size_px: child_width.min_needed_space().unwrap_or(0.0),
                    };
                    let child_flex_basis = child
                        .$get_flex_basis(parent_available_space)
                        .min(child_width.max_available_space().unwrap_or(core::f32::MAX));
                    child.min_inner_size_px = child.min_inner_size_px.max(child_flex_basis);
                    new_nodes.as_ref_mut()[child_id] = child;
                }
            }

            new_nodes
        }

        /// Bubble the inner sizes to their parents -  on any parent nodes, fill out
        /// the width so that the `preferred_width` can contain the child nodes (if
        /// that doesn't violate the constraints of the parent)
        fn $bubble_fn_name<'a, 'b>(
            node_data: &mut NodeDataContainerRefMut<'b, $struct_name>,
            node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
            layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
            layout_directions: &NodeDataContainerRef<'a, LayoutFlexDirection>,
            wh_configs: &NodeDataContainerRef<'a, WhConfig>,
            node_depths: &[ParentWithNodeDepth],
            root_size_width: f32,
        ) {
            // Reverse, since we want to go from the inside out
            // (depth 5 needs to be filled out first)
            //
            // Set the preferred_width of the parent nodes
            for ParentWithNodeDepth { depth: _, node_id } in node_depths.iter().rev() {
                let parent_id = match node_id.into_crate_internal() {
                    Some(s) => s,
                    None => continue,
                };

                let parent_parent_width = match node_hierarchy[parent_id].parent_id() {
                    None => root_size_width,
                    Some(s) => node_data[s]
                        .$preferred_field
                        .max_available_space()
                        .unwrap_or(root_size_width), // TODO: wrong
                };

                let parent_width = node_data[parent_id]
                    .$preferred_field
                    .max_available_space()
                    .unwrap_or(parent_parent_width);
                let flex_axis = layout_directions[parent_id].get_axis();

                let mut children_flex_basis = 0.0_f32;

                parent_id
                    .az_children(node_hierarchy)
                    .filter(|child_id| layout_positions[*child_id] != LayoutPosition::Absolute)
                    .map(|child_id| {
                        (
                            child_id,
                            node_data[child_id].min_inner_size_px
                                + node_data[child_id].$get_margin_fn(parent_width),
                        )
                    })
                    .for_each(|(_, flex_basis)| {
                        if flex_axis == LayoutAxis::$main_axis {
                            children_flex_basis += flex_basis;
                        } else {
                            // cross direction: take max flex basis of children
                            children_flex_basis = children_flex_basis.max(flex_basis);
                        }
                    });

                // if the children overflow, then the maximum width / height that can be
                // bubbled is the max_height / max_width of the parent
                let parent_max_available_space = node_data[parent_id]
                    .$preferred_field
                    .max_available_space()
                    .unwrap_or(children_flex_basis);
                let children_inner_width = parent_max_available_space.min(children_flex_basis);

                // parent minimum width = children (including borders, padding + margin of children)
                // PLUS padding (including borders) of parent
                let parent_min_inner_size_px = children_inner_width
                    + node_data[parent_id].$get_padding_fn(parent_parent_width);

                // bubble the min_inner_size_px to the parent
                node_data[parent_id].min_inner_size_px = node_data[parent_id]
                    .min_inner_size_px
                    .max(parent_min_inner_size_px);
            }

            // Now, the width of all elements should be filled,
            // but they aren't flex-grown yet
        }

        /// Go from the root down and flex_grow the children if
        /// needed - respects the `width`, `min_width` and `max_width`
        /// properties
        ///
        /// The layout step doesn't account for the min_width
        /// and max_width constraints, so we have to adjust them manually
        fn $apply_flex_grow_fn_name<'a, 'b>(
            node_data: &mut NodeDataContainer<$struct_name>,
            node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
            layout_displays: &NodeDataContainerRef<'a, CssPropertyValue<LayoutDisplay>>,
            layout_flex_grows: &NodeDataContainerRef<'a, f32>,
            layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
            layout_directions: &NodeDataContainerRef<'a, LayoutFlexDirection>,
            node_depths: &[ParentWithNodeDepth],
            root_width: f32,
            parents_to_recalc: &BTreeSet<NodeId>,
        ) {
            /// Does the actual width layout, respects the `width`,
            /// `min_width` and `max_width` properties as well as the
            /// `flex_grow` factor. `flex_shrink` currently does nothing.
            fn distribute_space_along_main_axis<'a>(
                node_id: &NodeId,
                children: &[NodeId],
                node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
                layout_displays: &NodeDataContainerRef<'a, CssPropertyValue<LayoutDisplay>>,
                layout_flex_grows: &NodeDataContainerRef<'a, f32>,
                layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
                width_calculated_arena: &'a NodeDataContainerRef<$struct_name>,
                root_width: f32,
            ) -> Vec<f32> {
                // The inner space of the parent node, without the padding
                let parent_node_inner_width = {
                    let parent_node = &width_calculated_arena[*node_id];
                    let parent_parent_width = node_hierarchy[*node_id]
                        .parent_id()
                        .and_then(|p| {
                            width_calculated_arena[p]
                                .$preferred_field
                                .max_available_space()
                        })
                        .unwrap_or(root_width);
                    parent_node.total() - parent_node.$get_padding_fn(parent_parent_width)
                };

                // 1. Set all child elements to their minimum required width or 0.0
                // if there is no min width
                let mut children_flex_grow = children
                    .iter()
                    .map(|child_id| {
                        if layout_positions[*child_id] != LayoutPosition::Absolute {
                            // so that node.min_width + node.flex_grow_px = exact_width
                            match width_calculated_arena[*child_id].$preferred_field {
                                WhConstraint::Between(min, _) => {
                                    if min > width_calculated_arena[*child_id].min_inner_size_px {
                                        min - width_calculated_arena[*child_id].min_inner_size_px
                                    } else {
                                        0.0
                                    }
                                }
                                WhConstraint::EqualTo(exact) => {
                                    exact - width_calculated_arena[*child_id].min_inner_size_px
                                }
                                WhConstraint::Unconstrained => 0.0,
                            }
                        } else {
                            // `position: absolute` items don't take space away from their siblings,
                            // rather they take the minimum needed space by
                            // their content
                            let nearest_relative_parent_node = child_id
                                .get_nearest_matching_parent(node_hierarchy, |n| {
                                    layout_positions[n].is_positioned()
                                })
                                .unwrap_or(NodeId::new(0));

                            let relative_parent_width = {
                                let relative_parent_node =
                                    &width_calculated_arena[nearest_relative_parent_node];
                                relative_parent_node.flex_grow_px
                                    + relative_parent_node.min_inner_size_px
                            };

                            // The absolute positioned node might have a max-width constraint, which
                            // has a higher precedence than `top, bottom, left,
                            // right`.
                            let max_space_current_node = width_calculated_arena[*child_id]
                                .$preferred_field
                                .calculate_from_relative_parent(relative_parent_width);

                            // expand so that node.min_inner_size_px + node.flex_grow_px =
                            // max_space_current_node
                            if max_space_current_node
                                > width_calculated_arena[*child_id].min_inner_size_px
                            {
                                max_space_current_node
                                    - width_calculated_arena[*child_id].min_inner_size_px
                            } else {
                                0.0
                            }
                        }
                    })
                    .collect::<Vec<f32>>();

                // 2. Calculate how much space has been taken up so far by the minimum width /
                //    height Exclude position: absolute items from being added into the sum since
                //    they are taken out of the regular layout flow
                let space_taken_up: f32 = children
                    .iter()
                    .enumerate()
                    .filter(|(_, child_id)| {
                        layout_positions[**child_id] != LayoutPosition::Absolute
                    })
                    .map(|(child_index_in_parent, child_id)| {
                        width_calculated_arena[*child_id].min_inner_size_px
                            + width_calculated_arena[*child_id]
                                .$get_margin_fn(parent_node_inner_width)
                            + children_flex_grow[child_index_in_parent]
                    })
                    .sum();

                // all items are now expanded to their minimum width,
                // calculate how much space is remaining
                let mut space_available = parent_node_inner_width - space_taken_up;

                if space_available <= 0.0 {
                    // no space to distribute
                    return children_flex_grow;
                }

                // The fixed-width items are now considered solved,
                // so subtract them out of the width of the parent.

                // Get the node ids that have to be expanded, exclude
                // fixed-width and absolute childrens
                let mut variable_width_childs = children
                    .iter()
                    .enumerate()
                    .filter(|(_, id)| {
                        !width_calculated_arena[**id]
                            .$preferred_field
                            .is_fixed_constraint()
                    })
                    .filter(|(_, id)| layout_positions[**id] != LayoutPosition::Absolute)
                    .filter(|(_, id)| {
                        !(layout_displays[**id] == CssPropertyValue::Exact(LayoutDisplay::None)
                            || layout_displays[**id] == CssPropertyValue::None)
                    })
                    .filter(|(_, id)| layout_flex_grows[**id] > 0.0)
                    .map(|(index_in_parent, id)| (*id, index_in_parent))
                    .collect::<BTreeMap<NodeId, usize>>();

                loop {
                    if !(space_available > 0.0) || variable_width_childs.is_empty() {
                        break;
                    }

                    // In order to apply flex-grow correctly, we need the sum of
                    // the flex-grow factors of all the variable-width children
                    //
                    // NOTE: variable_width_childs can change its length,
                    // have to recalculate every loop!
                    let children_combined_flex_grow: f32 = variable_width_childs
                        .iter()
                        .map(|(child_id, _)| layout_flex_grows[*child_id])
                        .sum();

                    if children_combined_flex_grow <= 0.0 {
                        break;
                    }

                    let size_per_child = space_available / children_combined_flex_grow;

                    // Grow all variable children by the same amount.
                    let new_iteration = variable_width_childs
                        .iter()
                        .map(|(variable_child_id, index_in_parent)| {
                            let flex_grow_of_child = layout_flex_grows[*variable_child_id];
                            let added_space_for_one_child = size_per_child * flex_grow_of_child;
                            let max_width = width_calculated_arena[*variable_child_id]
                                .$preferred_field
                                .max_available_space();
                            let current_flex_grow = children_flex_grow[*index_in_parent];
                            let current_width_of_child = {
                                width_calculated_arena[*variable_child_id].min_inner_size_px
                                    + current_flex_grow
                            };

                            let (flex_grow_this_iteration, node_is_solved) = match max_width {
                                Some(max) => {
                                    let overflow: f32 =
                                        current_width_of_child + added_space_for_one_child - max;
                                    if !overflow.is_sign_negative() {
                                        // flex-growing will overflow max-width, record overflow and
                                        // set
                                        ((max - current_width_of_child).max(0.0), true)
                                    } else {
                                        (added_space_for_one_child, false)
                                    }
                                }
                                None => (added_space_for_one_child, false),
                            };

                            (
                                *variable_child_id,
                                *index_in_parent,
                                flex_grow_this_iteration,
                                node_is_solved,
                            )
                        })
                        .collect::<Vec<_>>();

                    for (child_id, index_in_parent, flex_grow_to_add, node_is_solved) in
                        new_iteration
                    {
                        children_flex_grow[index_in_parent] += flex_grow_to_add;
                        space_available -= flex_grow_to_add;
                        if node_is_solved {
                            variable_width_childs.remove(&child_id);
                        }
                    }
                }

                children_flex_grow
            }

            fn distribute_space_along_cross_axis<'a>(
                parent_id: &NodeId,
                children: &[NodeId],
                node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
                layout_displays: &NodeDataContainerRef<'a, CssPropertyValue<LayoutDisplay>>,
                layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
                width_calculated_arena: &'a NodeDataContainerRef<$struct_name>,
                root_width: f32,
            ) -> Vec<f32> {
                let parent_node_inner_width = {
                    // The inner space of the parent node, without the padding
                    let parent_node = &width_calculated_arena[*parent_id];
                    let parent_parent_width = node_hierarchy[*parent_id]
                        .parent_id()
                        .and_then(|p| {
                            width_calculated_arena[p]
                                .$preferred_field
                                .max_available_space()
                        })
                        .unwrap_or(root_width);

                    parent_node.total() - parent_node.$get_padding_fn(parent_parent_width)
                };

                let nearest_relative_node = if layout_positions[*parent_id].is_positioned() {
                    *parent_id
                } else {
                    parent_id
                        .get_nearest_matching_parent(node_hierarchy, |n| {
                            layout_positions[n].is_positioned()
                        })
                        .unwrap_or(NodeId::new(0))
                };

                let last_relative_node_inner_width = {
                    let last_relative_node = &width_calculated_arena[nearest_relative_node];
                    let last_relative_node_parent_width = node_hierarchy[nearest_relative_node]
                        .parent_id()
                        .and_then(|p| {
                            width_calculated_arena[p]
                                .$preferred_field
                                .max_available_space()
                        })
                        .unwrap_or(root_width);

                    last_relative_node.total()
                        - last_relative_node.$get_padding_fn(last_relative_node_parent_width)
                };

                children
                    .iter()
                    .map(|child_id| {
                        let parent_node_inner_width =
                            if layout_positions[*child_id] == LayoutPosition::Absolute {
                                last_relative_node_inner_width
                            } else {
                                parent_node_inner_width
                            };

                        let min_child_width = width_calculated_arena[*child_id].total(); // +
                                                                                         // width_calculated_arena[*child_id].
                                                                                         // $get_padding_fn(parent_node_inner_width);
                                                                                         // + margin(child)

                        let space_available = parent_node_inner_width - min_child_width;

                        // If the min width of the cross axis is larger than the parent width,
                        // overflow
                        if space_available <= 0.0
                            || layout_displays[*child_id]
                                .get_property()
                                .copied()
                                .unwrap_or_default()
                                != LayoutDisplay::Flex
                        {
                            // do not grow the item - no space to distribute
                            0.0
                        } else {
                            let preferred_width = match width_calculated_arena[*child_id]
                                .$preferred_field
                                .max_available_space()
                            {
                                Some(max_width) => parent_node_inner_width.min(max_width),
                                None => parent_node_inner_width,
                            };
                            // flex_grow the item so that (space_available + node.flex_grow_px) =
                            // preferred_width (= either max_width or parent_width)
                            preferred_width - min_child_width
                        }
                    })
                    .collect()
            }

            use azul_css::{LayoutAxis, LayoutPosition};

            // Set the window width on the root node (since there is only one root node, we can
            // calculate the `flex_grow_px` directly)
            //
            // Usually `top_level_flex_basis` is NOT 0.0, rather it's the sum of all widths in the
            // DOM, i.e. the sum of the whole DOM tree
            let top_level_flex_basis = node_data.as_ref()[NodeId::ZERO].min_inner_size_px;

            // The root node can still have some sort of max-width attached, so we need to check for
            // that
            let root_preferred_width = match node_data.as_ref()[NodeId::ZERO]
                .$preferred_field
                .max_available_space()
            {
                Some(max_width) => root_width.min(max_width),
                None => root_width,
            };

            node_data.as_ref_mut()[NodeId::ZERO].flex_grow_px =
                root_preferred_width - top_level_flex_basis;

            let mut parents_grouped_by_depth = BTreeMap::new();
            for ParentWithNodeDepth { depth, node_id } in node_depths.iter() {
                let parent_id = match node_id.into_crate_internal() {
                    Some(s) => s,
                    None => continue,
                };
                if !parents_to_recalc.contains(&parent_id) {
                    continue;
                }
                parents_grouped_by_depth
                    .entry(depth)
                    .or_insert_with(|| Vec::new())
                    .push(parent_id);
            }

            for (depth, parent_ids) in parents_grouped_by_depth {
                // reset the flex_grow to 0
                {
                    let mut node_data_mut = node_data.as_ref_mut();
                    for parent_id in parent_ids.iter() {
                        for child_id in parent_id.az_children(node_hierarchy) {
                            node_data_mut[child_id].flex_grow_px = 0.0;
                        }
                    }
                }

                // calculate the new flex_grow
                let flex_grows_in_this_depth = parent_ids
                    .iter()
                    .map(|parent_id| {
                        let children = parent_id.az_children_collect(&node_hierarchy);
                        let flex_axis = layout_directions[*parent_id].get_axis();

                        let result = if flex_axis == LayoutAxis::$main_axis {
                            distribute_space_along_main_axis(
                                &parent_id,
                                &children,
                                node_hierarchy,
                                layout_displays,
                                layout_flex_grows,
                                layout_positions,
                                &node_data.as_ref(),
                                root_width,
                            )
                        } else {
                            distribute_space_along_cross_axis(
                                &parent_id,
                                &children,
                                node_hierarchy,
                                layout_displays,
                                layout_positions,
                                &node_data.as_ref(),
                                root_width,
                            )
                        };

                        (parent_id, result)
                    })
                    .collect::<Vec<_>>();

                // write the new flex-grow values into the flex_grow_px
                {
                    let mut node_data_mut = node_data.as_ref_mut();
                    for (parent_id, flex_grows) in flex_grows_in_this_depth {
                        for (child_id, flex_grow_px) in parent_id
                            .az_children(node_hierarchy)
                            .zip(flex_grows.into_iter())
                        {
                            node_data_mut[child_id].flex_grow_px = flex_grow_px;
                        }
                    }
                }
            }
        }
    };
}

typed_arena!(
    WidthCalculatedRect,
    preferred_width,
    determine_preferred_width,
    width,
    get_horizontal_padding,
    get_horizontal_border,
    get_horizontal_margin,
    get_flex_basis_horizontal,
    width_calculated_rect_arena_from_rect_layout_arena,
    bubble_preferred_widths_to_parents,
    width_calculated_rect_arena_apply_flex_grow,
    Horizontal,
    margin_left,
    margin_right,
    padding_left,
    padding_right,
    border_left,
    border_right,
    left,
    right,
);

typed_arena!(
    HeightCalculatedRect,
    preferred_height,
    determine_preferred_height,
    height,
    get_vertical_padding,
    get_vertical_border,
    get_vertical_margin,
    get_flex_basis_vertical,
    height_calculated_rect_arena_from_rect_layout_arena,
    bubble_preferred_heights_to_parents,
    height_calculated_rect_arena_apply_flex_grow,
    Vertical,
    margin_top,
    margin_bottom,
    padding_top,
    padding_bottom,
    border_top,
    border_bottom,
    top,
    bottom,
);

/// Returns the solved widths of the items in a BTree form
pub(crate) fn solve_flex_layout_width<'a, 'b>(
    width_calculated_arena: &'a mut NodeDataContainer<WidthCalculatedRect>,
    layout_flex_grow: &NodeDataContainerRef<'a, f32>,
    layout_displays: &NodeDataContainerRef<'a, CssPropertyValue<LayoutDisplay>>,
    layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
    layout_directions: &NodeDataContainerRef<'a, LayoutFlexDirection>,
    node_hierarchy: &'b NodeDataContainerRef<'a, NodeHierarchyItem>,
    wh_configs: &NodeDataContainerRef<'a, WhConfig>,
    node_depths: &[ParentWithNodeDepth],
    window_width: f32,
    parents_to_recalc: &BTreeSet<NodeId>,
) {
    bubble_preferred_widths_to_parents(
        &mut width_calculated_arena.as_ref_mut(),
        node_hierarchy,
        layout_positions,
        layout_directions,
        wh_configs,
        node_depths,
        window_width,
    );
    width_calculated_rect_arena_apply_flex_grow(
        width_calculated_arena,
        node_hierarchy,
        layout_displays,
        layout_flex_grow,
        layout_positions,
        layout_directions,
        node_depths,
        window_width,
        parents_to_recalc,
    );
}

/// Returns the solved height of the items in a BTree form
pub(crate) fn solve_flex_layout_height<'a, 'b>(
    height_calculated_arena: &'a mut NodeDataContainer<HeightCalculatedRect>,
    layout_flex_grow: &NodeDataContainerRef<'a, f32>,
    layout_displays: &NodeDataContainerRef<'a, CssPropertyValue<LayoutDisplay>>,
    layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
    layout_directions: &NodeDataContainerRef<'a, LayoutFlexDirection>,
    node_hierarchy: &'b NodeDataContainerRef<'a, NodeHierarchyItem>,
    wh_configs: &NodeDataContainerRef<'a, WhConfig>,
    node_depths: &[ParentWithNodeDepth],
    window_height: f32,
    parents_to_recalc: &BTreeSet<NodeId>,
) {
    bubble_preferred_heights_to_parents(
        &mut height_calculated_arena.as_ref_mut(),
        node_hierarchy,
        layout_positions,
        layout_directions,
        wh_configs,
        node_depths,
        window_height,
    );
    height_calculated_rect_arena_apply_flex_grow(
        height_calculated_arena,
        node_hierarchy,
        layout_displays,
        layout_flex_grow,
        layout_positions,
        layout_directions,
        node_depths,
        window_height,
        parents_to_recalc,
    );
}

macro_rules! get_position {
    (
        $fn_name:ident,
        $width_layout:ident,
        $height_solved_position:ident,
        $solved_widths_field:ident,
        $left:ident,
        $right:ident,
        $margin_left:ident,
        $margin_right:ident,
        $get_padding_left:ident,
        $get_padding_right:ident,
        $axis:ident
    ) => {
        /// Traverses along the DOM and solve for the X or Y position
        fn $fn_name<'a>(
            arena: &mut NodeDataContainer<$height_solved_position>,
            node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
            layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
            layout_directions: &NodeDataContainerRef<'a, LayoutFlexDirection>,
            layout_justify_contents: &NodeDataContainerRef<'a, LayoutJustifyContent>,
            node_depths: &[ParentWithNodeDepth],
            solved_widths: &NodeDataContainerRef<'a, $width_layout>,
            parents_to_solve: &BTreeSet<NodeId>,
        ) {
            /// Returns the absolute X for the child
            fn determine_child_x_absolute<'a>(
                child_id: NodeId,
                solved_widths: &NodeDataContainerRef<'a, $width_layout>,
                layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
                node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
            ) -> f32 {
                let child_width_with_padding = {
                    let child_node = &solved_widths[child_id];
                    child_node.min_inner_size_px + child_node.flex_grow_px
                };

                let child_node = &solved_widths[child_id];
                let child_node_parent_width = node_hierarchy[child_id]
                    .parent_id()
                    .map(|p| solved_widths[p].total())
                    .unwrap_or(0.0) as f32;

                let child_right = child_node
                    .$right
                    .and_then(|s| Some(s.get_property()?.inner.to_pixels(child_node_parent_width)));

                if let Some(child_right) = child_right {
                    // align right / bottom of last relative parent
                    let child_margin_right = child_node
                        .$margin_right
                        .and_then(|x| {
                            Some(x.get_property()?.inner.to_pixels(child_node_parent_width))
                        })
                        .unwrap_or(0.0);

                    let last_relative_node_id = child_id
                        .get_nearest_matching_parent(node_hierarchy, |n| {
                            layout_positions[n].is_positioned()
                        })
                        .unwrap_or(NodeId::new(0));

                    let last_relative_node_outer_width =
                        &solved_widths[last_relative_node_id].total();

                    last_relative_node_outer_width
                        - child_width_with_padding
                        - child_margin_right
                        - child_right
                } else {
                    // align left / top of last relative parent
                    let child_left = child_node.$left.and_then(|s| {
                        Some(s.get_property()?.inner.to_pixels(child_node_parent_width))
                    });

                    let child_margin_left = child_node
                        .$margin_left
                        .and_then(|x| {
                            Some(x.get_property()?.inner.to_pixels(child_node_parent_width))
                        })
                        .unwrap_or(0.0);

                    child_margin_left + child_left.unwrap_or(0.0)
                }
            }

            // Returns the X for the child + the distance to add for the next child
            fn determine_child_x_along_main_axis<'a>(
                main_axis_alignment: LayoutJustifyContent,
                layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
                solved_widths: &NodeDataContainerRef<'a, $width_layout>,
                child_id: NodeId,
                parent_x_position: f32,
                parent_inner_width: f32,
                sum_x_of_children_so_far: &f32,
                node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
            ) -> (f32, f32) {
                use azul_css::LayoutJustifyContent::*;

                // total width of the child, including padding + border
                let child_width_with_padding = solved_widths[child_id].total();

                // width: increase X according to the main axis, Y according to the cross_axis
                let child_node = &solved_widths[child_id];
                let child_margin_left = child_node
                    .$margin_left
                    .and_then(|x| Some(x.get_property()?.inner.to_pixels(parent_inner_width)))
                    .unwrap_or(0.0);
                let child_margin_right = child_node
                    .$margin_right
                    .and_then(|x| Some(x.get_property()?.inner.to_pixels(parent_inner_width)))
                    .unwrap_or(0.0);

                if layout_positions[child_id] == LayoutPosition::Absolute {
                    (
                        determine_child_x_absolute(
                            child_id,
                            solved_widths,
                            layout_positions,
                            node_hierarchy,
                        ),
                        0.0,
                    )
                } else {
                    // X position of the top left corner
                    // WARNING: End has to be added after all children!
                    let x_of_top_left_corner = match main_axis_alignment {
                        Start | End => {
                            parent_x_position + *sum_x_of_children_so_far + child_margin_left
                        }
                        Center => {
                            parent_x_position
                                + ((parent_inner_width as f32 / 2.0)
                                    - ((*sum_x_of_children_so_far
                                        + child_margin_right
                                        + child_width_with_padding)
                                        as f32
                                        / 2.0))
                        }
                        SpaceBetween => {
                            parent_x_position // TODO!
                        }
                        SpaceAround => {
                            parent_x_position // TODO!
                        }
                        SpaceEvenly => {
                            parent_x_position // TODO!
                        }
                    };

                    (
                        x_of_top_left_corner,
                        child_margin_right + child_width_with_padding + child_margin_left,
                    )
                }
            }

            fn determine_child_x_along_cross_axis<'a>(
                layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
                solved_widths: &NodeDataContainerRef<'a, $width_layout>,
                child_id: NodeId,
                parent_x_position: f32,
                parent_inner_width: f32,
                node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
            ) -> f32 {
                let child_node = &solved_widths[child_id];

                let child_margin_left = child_node
                    .$margin_left
                    .and_then(|x| Some(x.get_property()?.inner.to_pixels(parent_inner_width)))
                    .unwrap_or(0.0);

                if layout_positions[child_id] == LayoutPosition::Absolute {
                    determine_child_x_absolute(
                        child_id,
                        solved_widths,
                        layout_positions,
                        node_hierarchy,
                    )
                } else {
                    parent_x_position + child_margin_left
                }
            }

            use azul_css::{LayoutAxis, LayoutJustifyContent::*};

            for ParentWithNodeDepth { depth: _, node_id } in node_depths.iter() {
                let parent_id = match node_id.into_crate_internal() {
                    Some(s) => s,
                    None => continue,
                };

                if !parents_to_solve.contains(&parent_id) {
                    continue;
                }

                let parent_node = &solved_widths[parent_id];
                let parent_parent_width = node_hierarchy[parent_id]
                    .parent_id()
                    .map(|p| solved_widths[p].total())
                    .unwrap_or(0.0) as f32;

                let parent_padding_left = parent_node.$get_padding_left(parent_parent_width);
                let parent_padding_right = parent_node.$get_padding_right(parent_parent_width);

                let parent_x_position = arena.as_ref()[parent_id].0 + parent_padding_left;
                let parent_direction = layout_directions[parent_id];

                let parent_inner_width =
                    { parent_node.total() - (parent_padding_left + parent_padding_right) };

                if parent_direction.get_axis() == LayoutAxis::$axis {
                    // Along main axis: Increase X with width of current element
                    let main_axis_alignment = layout_justify_contents[parent_id];
                    let mut sum_x_of_children_so_far = 0.0;

                    if parent_direction.is_reverse() {
                        for child_id in parent_id.az_reverse_children(node_hierarchy) {
                            let (x, x_to_add) = determine_child_x_along_main_axis(
                                main_axis_alignment,
                                layout_positions,
                                solved_widths,
                                child_id,
                                parent_x_position,
                                parent_inner_width,
                                &sum_x_of_children_so_far,
                                node_hierarchy,
                            );
                            arena.as_ref_mut()[child_id].0 = x;
                            sum_x_of_children_so_far += x_to_add;
                        }
                    } else {
                        for child_id in parent_id.az_children(node_hierarchy) {
                            let (x, x_to_add) = determine_child_x_along_main_axis(
                                main_axis_alignment,
                                layout_positions,
                                solved_widths,
                                child_id,
                                parent_x_position,
                                parent_inner_width,
                                &sum_x_of_children_so_far,
                                node_hierarchy,
                            );
                            arena.as_ref_mut()[child_id].0 = x;
                            sum_x_of_children_so_far += x_to_add;
                        }
                    }

                    // If the direction is `flex-end`, we can't add the X position during the
                    // iteration, so we have to "add" the diff to the parent_inner_width
                    // at the end
                    let should_align_towards_end = (parent_direction.is_reverse()
                        && main_axis_alignment == Start)
                        || (!parent_direction.is_reverse() && main_axis_alignment == End);

                    if should_align_towards_end {
                        let diff = parent_inner_width - sum_x_of_children_so_far;
                        for child_id in parent_id
                            .az_children(node_hierarchy)
                            .filter(|ch| layout_positions[*ch] != LayoutPosition::Absolute)
                        {
                            arena.as_ref_mut()[child_id].0 += diff;
                        }
                    }
                } else {
                    // Along cross axis: Take X of parent

                    if parent_direction.is_reverse() {
                        for child_id in parent_id.az_reverse_children(node_hierarchy) {
                            arena.as_ref_mut()[child_id].0 = determine_child_x_along_cross_axis(
                                layout_positions,
                                solved_widths,
                                child_id,
                                parent_x_position,
                                parent_inner_width,
                                node_hierarchy,
                            );
                        }
                    } else {
                        for child_id in parent_id.az_children(node_hierarchy) {
                            arena.as_ref_mut()[child_id].0 = determine_child_x_along_cross_axis(
                                layout_positions,
                                solved_widths,
                                child_id,
                                parent_x_position,
                                parent_inner_width,
                                node_hierarchy,
                            );
                        }
                    }
                }
            }
        }
    };
}

fn get_x_positions<'a>(
    arena: &mut NodeDataContainer<HorizontalSolvedPosition>,
    solved_widths: &NodeDataContainerRef<'a, WidthCalculatedRect>,
    node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
    layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
    layout_directions: &NodeDataContainerRef<'a, LayoutFlexDirection>,
    layout_justify_contents: &NodeDataContainerRef<'a, LayoutJustifyContent>,
    node_depths: &[ParentWithNodeDepth],
    origin: LogicalPosition,
    parents_to_solve: &BTreeSet<NodeId>,
) {
    get_position!(
        get_pos_x,
        WidthCalculatedRect,
        HorizontalSolvedPosition,
        solved_widths,
        left,
        right,
        margin_left,
        margin_right,
        get_padding_left,
        get_padding_right,
        Horizontal
    );

    get_pos_x(
        arena,
        node_hierarchy,
        layout_positions,
        layout_directions,
        layout_justify_contents,
        node_depths,
        solved_widths,
        &parents_to_solve,
    );

    // Add the origin on top of the position
    for item in arena.internal.iter_mut() {
        item.0 += origin.x;
    }
}

fn get_y_positions<'a>(
    arena: &mut NodeDataContainer<VerticalSolvedPosition>,
    solved_heights: &NodeDataContainerRef<'a, HeightCalculatedRect>,
    node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
    layout_positions: &NodeDataContainerRef<'a, LayoutPosition>,
    layout_directions: &NodeDataContainerRef<'a, LayoutFlexDirection>,
    layout_justify_contents: &NodeDataContainerRef<'a, LayoutJustifyContent>,
    node_depths: &[ParentWithNodeDepth],
    origin: LogicalPosition,
    parents_to_solve: &BTreeSet<NodeId>,
) {
    get_position!(
        get_pos_y,
        HeightCalculatedRect,
        VerticalSolvedPosition,
        solved_heights,
        top,
        bottom,
        margin_top,
        margin_bottom,
        get_padding_top,
        get_padding_bottom,
        Vertical
    );

    get_pos_y(
        arena,
        node_hierarchy,
        layout_positions,
        layout_directions,
        layout_justify_contents,
        node_depths,
        solved_heights,
        &parents_to_solve,
    );

    // Add the origin on top of the position
    for item in arena.internal.iter_mut() {
        item.0 += origin.y;
    }
}

#[inline]
pub fn get_layout_positions<'a>(styled_dom: &StyledDom) -> NodeDataContainer<LayoutPosition> {
    let cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();
    assert!(node_data_container.internal.len() == styled_nodes.internal.len()); // elide bounds checking
    NodeDataContainer {
        internal: styled_nodes
            .internal
            .iter()
            .enumerate()
            .map(|(node_id, styled_node)| {
                cache
                    .get_position(
                        &node_data_container.internal[node_id],
                        &NodeId::new(node_id),
                        &styled_node.state,
                    )
                    .cloned()
                    .unwrap_or_default()
                    .get_property_or_default()
                    .unwrap_or_default()
            })
            .collect(),
    }
}

#[inline]
pub fn get_layout_justify_contents<'a>(
    styled_dom: &StyledDom,
) -> NodeDataContainer<LayoutJustifyContent> {
    let cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();
    assert!(node_data_container.internal.len() == styled_nodes.internal.len()); // elide bounds checking

    NodeDataContainer {
        internal: styled_nodes
            .internal
            .iter()
            .enumerate()
            .map(|(node_id, styled_node)| {
                cache
                    .get_justify_content(
                        &node_data_container.internal[node_id],
                        &NodeId::new(node_id),
                        &styled_node.state,
                    )
                    .cloned()
                    .unwrap_or_default()
                    .get_property_or_default()
                    .unwrap_or_default()
            })
            .collect(),
    }
}

#[inline]
pub fn get_layout_flex_directions<'a>(
    styled_dom: &StyledDom,
) -> NodeDataContainer<LayoutFlexDirection> {
    let cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();
    assert!(node_data_container.internal.len() == styled_nodes.internal.len()); // elide bounds checking

    NodeDataContainer {
        internal: styled_nodes
            .internal
            .iter()
            .enumerate()
            .map(|(node_id, styled_node)| {
                cache
                    .get_flex_direction(
                        &node_data_container.internal[node_id],
                        &NodeId::new(node_id),
                        &styled_node.state,
                    )
                    .cloned()
                    .unwrap_or_default()
                    .get_property_or_default()
                    .unwrap_or_default()
            })
            .collect(),
    }
}

#[inline]
pub fn get_layout_flex_grows<'a>(styled_dom: &StyledDom) -> NodeDataContainer<f32> {
    // Prevent flex-grow and flex-shrink to be less than 0
    let cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();
    assert!(node_data_container.internal.len() == styled_nodes.internal.len()); // elide bounds checking

    NodeDataContainer {
        internal: styled_nodes
            .internal
            .iter()
            .enumerate()
            .map(|(node_id, styled_node)| {
                cache
                    .get_flex_grow(
                        &node_data_container.internal[node_id],
                        &NodeId::new(node_id),
                        &styled_node.state,
                    )
                    .and_then(|g| g.get_property().copied())
                    .and_then(|grow| Some(grow.inner.get().max(0.0)))
                    .unwrap_or(DEFAULT_FLEX_GROW_FACTOR)
            })
            .collect(),
    }
}

#[inline]
pub fn get_layout_displays<'a>(
    styled_dom: &StyledDom,
) -> NodeDataContainer<CssPropertyValue<LayoutDisplay>> {
    // Prevent flex-grow and flex-shrink to be less than 0
    let cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();
    assert!(node_data_container.internal.len() == styled_nodes.internal.len()); // elide bounds checking

    NodeDataContainer {
        internal: styled_nodes
            .internal
            .iter()
            .enumerate()
            .map(|(node_id, styled_node)| {
                cache
                    .get_display(
                        &node_data_container.internal[node_id],
                        &NodeId::new(node_id),
                        &styled_node.state,
                    )
                    .copied()
                    .unwrap_or(CssPropertyValue::Auto)
            })
            .collect(),
    }
}

fn precalculate_all_offsets(styled_dom: &StyledDom) -> NodeDataContainer<AllOffsets> {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();
    assert!(styled_nodes.internal.len() == node_data_container.internal.len()); // elide bounds check

    NodeDataContainer {
        internal: styled_nodes
            .internal
            .iter()
            .enumerate()
            .map(|(node_id_usize, styled_node)| {
                let node_id = NodeId::new(node_id_usize);
                let state = &styled_node.state;
                precalculate_offset(
                    &node_data_container.internal[node_id_usize],
                    &css_property_cache,
                    &node_id,
                    state,
                )
            })
            .collect(),
    }
}

struct AllOffsets {
    position: LayoutAbsolutePositions,
    border_widths: LayoutBorderOffsets,
    padding: LayoutPaddingOffsets,
    margin: LayoutMarginOffsets,
    box_shadow: StyleBoxShadowOffsets,
    box_sizing: LayoutBoxSizing,
    overflow_x: LayoutOverflow,
    overflow_y: LayoutOverflow,
}

fn precalculate_offset(
    node_data: &NodeData,
    css_property_cache: &CssPropertyCache,
    node_id: &NodeId,
    state: &StyledNodeState,
) -> AllOffsets {
    AllOffsets {
        border_widths: LayoutBorderOffsets {
            left: css_property_cache
                .get_border_left_width(node_data, node_id, state)
                .cloned(),
            right: css_property_cache
                .get_border_right_width(node_data, node_id, state)
                .cloned(),
            top: css_property_cache
                .get_border_top_width(node_data, node_id, state)
                .cloned(),
            bottom: css_property_cache
                .get_border_bottom_width(node_data, node_id, state)
                .cloned(),
        },
        padding: LayoutPaddingOffsets {
            left: css_property_cache
                .get_padding_left(node_data, node_id, state)
                .cloned(),
            right: css_property_cache
                .get_padding_right(node_data, node_id, state)
                .cloned(),
            top: css_property_cache
                .get_padding_top(node_data, node_id, state)
                .cloned(),
            bottom: css_property_cache
                .get_padding_bottom(node_data, node_id, state)
                .cloned(),
        },
        margin: LayoutMarginOffsets {
            left: css_property_cache
                .get_margin_left(node_data, node_id, state)
                .cloned(),
            right: css_property_cache
                .get_margin_right(node_data, node_id, state)
                .cloned(),
            top: css_property_cache
                .get_margin_top(node_data, node_id, state)
                .cloned(),
            bottom: css_property_cache
                .get_margin_bottom(node_data, node_id, state)
                .cloned(),
        },
        box_shadow: StyleBoxShadowOffsets {
            left: css_property_cache
                .get_box_shadow_left(node_data, node_id, state)
                .cloned(),
            right: css_property_cache
                .get_box_shadow_right(node_data, node_id, state)
                .cloned(),
            top: css_property_cache
                .get_box_shadow_top(node_data, node_id, state)
                .cloned(),
            bottom: css_property_cache
                .get_box_shadow_bottom(node_data, node_id, state)
                .cloned(),
        },
        position: LayoutAbsolutePositions {
            left: css_property_cache
                .get_left(node_data, node_id, state)
                .cloned(),
            right: css_property_cache
                .get_right(node_data, node_id, state)
                .cloned(),
            top: css_property_cache
                .get_top(node_data, node_id, state)
                .cloned(),
            bottom: css_property_cache
                .get_bottom(node_data, node_id, state)
                .cloned(),
        },
        box_sizing: css_property_cache
            .get_box_sizing(node_data, node_id, state)
            .cloned()
            .unwrap_or_default()
            .get_property()
            .copied()
            .unwrap_or_default(),
        overflow_x: css_property_cache
            .get_overflow_x(node_data, node_id, state)
            .cloned()
            .unwrap_or_default()
            .get_property()
            .copied()
            .unwrap_or_default(),
        overflow_y: css_property_cache
            .get_overflow_y(node_data, node_id, state)
            .cloned()
            .unwrap_or_default()
            .get_property()
            .copied()
            .unwrap_or_default(),
    }
}

struct LayoutAbsolutePositions {
    left: Option<CssPropertyValue<LayoutLeft>>,
    right: Option<CssPropertyValue<LayoutRight>>,
    top: Option<CssPropertyValue<LayoutTop>>,
    bottom: Option<CssPropertyValue<LayoutBottom>>,
}

struct LayoutBorderOffsets {
    left: Option<CssPropertyValue<LayoutBorderLeftWidth>>,
    right: Option<CssPropertyValue<LayoutBorderRightWidth>>,
    top: Option<CssPropertyValue<LayoutBorderTopWidth>>,
    bottom: Option<CssPropertyValue<LayoutBorderBottomWidth>>,
}

impl LayoutBorderOffsets {
    fn resolve(&self, parent_scale_x: f32, parent_scale_y: f32) -> ResolvedOffsets {
        ResolvedOffsets {
            left: self
                .left
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_x)))
                .unwrap_or_default(),
            top: self
                .top
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_y)))
                .unwrap_or_default(),
            bottom: self
                .bottom
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_y)))
                .unwrap_or_default(),
            right: self
                .right
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_x)))
                .unwrap_or_default(),
        }
    }
}

struct LayoutPaddingOffsets {
    left: Option<CssPropertyValue<LayoutPaddingLeft>>,
    right: Option<CssPropertyValue<LayoutPaddingRight>>,
    top: Option<CssPropertyValue<LayoutPaddingTop>>,
    bottom: Option<CssPropertyValue<LayoutPaddingBottom>>,
}

impl LayoutPaddingOffsets {
    fn resolve(&self, parent_scale_x: f32, parent_scale_y: f32) -> ResolvedOffsets {
        ResolvedOffsets {
            left: self
                .left
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_x)))
                .unwrap_or_default(),
            top: self
                .top
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_y)))
                .unwrap_or_default(),
            bottom: self
                .bottom
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_y)))
                .unwrap_or_default(),
            right: self
                .right
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_x)))
                .unwrap_or_default(),
        }
    }
}

struct LayoutMarginOffsets {
    left: Option<CssPropertyValue<LayoutMarginLeft>>,
    right: Option<CssPropertyValue<LayoutMarginRight>>,
    top: Option<CssPropertyValue<LayoutMarginTop>>,
    bottom: Option<CssPropertyValue<LayoutMarginBottom>>,
}

impl LayoutMarginOffsets {
    fn resolve(&self, parent_scale_x: f32, parent_scale_y: f32) -> ResolvedOffsets {
        ResolvedOffsets {
            left: self
                .left
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_x)))
                .unwrap_or_default(),
            top: self
                .top
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_y)))
                .unwrap_or_default(),
            bottom: self
                .bottom
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_y)))
                .unwrap_or_default(),
            right: self
                .right
                .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_scale_x)))
                .unwrap_or_default(),
        }
    }
}

struct NewIframeScrollState {
    dom_id: DomId,
    node_id: NodeId,
    child_rect: LogicalRect,
    virtual_child_rect: LogicalRect,
}

// Adds the image and font resources to the app_resources but does NOT add them to the RenderAPI
#[cfg(feature = "text_layout")]
pub fn do_the_layout(
    styled_dom: StyledDom,
    image_cache: &ImageCache,
    fc_cache: &FcFontCache,
    renderer_resources: &mut RendererResources,
    current_window_dpi: DpiScaleFactor,
    all_resource_updates: &mut Vec<ResourceUpdate>,
    id_namespace: IdNamespace,
    document_id: &DocumentId,
    epoch: Epoch,
    callbacks: &RenderCallbacks,
    full_window_state: &FullWindowState,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Vec<LayoutResult> {
    use azul_core::{
        callbacks::{HidpiAdjustedBounds, IFrameCallbackInfo, IFrameCallbackReturn},
        styled_dom::NodeHierarchyItemId,
        ui_solver::{LayoutDebugMessage, OverflowingScrollNode},
    };

    if let Some(messages) = debug_messages.as_mut() {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Starting layout for window size: {:?}",
                full_window_state.size.dimensions
            )
            .into(),
            location: "do_the_layout".to_string().into(),
        });
    }

    let window_theme = full_window_state.theme;
    let mut current_dom_id = 0;
    let mut doms = vec![(
        None,
        DomId {
            inner: current_dom_id,
        },
        styled_dom,
        LogicalRect::new(LogicalPosition::zero(), full_window_state.size.dimensions),
    )];
    let mut resolved_doms = Vec::new();
    let mut new_scroll_states = Vec::new();

    loop {
        let mut new_doms = Vec::new();

        for (parent_dom_id, dom_id, styled_dom, rect) in doms.drain(..) {
            use azul_core::app_resources::add_fonts_and_images;

            add_fonts_and_images(
                image_cache,
                renderer_resources,
                current_window_dpi,
                fc_cache,
                id_namespace,
                epoch,
                document_id,
                all_resource_updates,
                &styled_dom,
                callbacks.load_font_fn,
                callbacks.parse_font_fn,
                callbacks.insert_into_active_gl_textures_fn,
            );

            let mut layout_result = do_the_layout_internal(
                dom_id,
                parent_dom_id,
                styled_dom,
                renderer_resources,
                document_id,
                rect,
                debug_messages,
            );

            let mut iframe_mapping = BTreeMap::new();

            for iframe_node_id in layout_result.styled_dom.scan_for_iframe_callbacks() {
                // Generate a new DomID
                current_dom_id += 1;
                let iframe_dom_id = DomId {
                    inner: current_dom_id,
                };
                iframe_mapping.insert(iframe_node_id, iframe_dom_id);

                let bounds = &layout_result.rects.as_ref()[iframe_node_id];
                let bounds_size = LayoutSize::new(
                    bounds.size.width.round() as isize,
                    bounds.size.height.round() as isize,
                );
                let hidpi_bounds = HidpiAdjustedBounds::from_bounds(
                    bounds_size,
                    full_window_state.size.get_hidpi_factor(),
                );

                // Invoke the IFrame callback
                let iframe_return: IFrameCallbackReturn = {
                    let mut iframe_callback_info = IFrameCallbackInfo::new(
                        fc_cache,
                        image_cache,
                        window_theme,
                        hidpi_bounds,
                        bounds.size,
                        LogicalPosition::zero(),
                        bounds.size,
                        LogicalPosition::zero(),
                    );

                    let mut node_data_mut = layout_result.styled_dom.node_data.as_container_mut();
                    match &mut node_data_mut[iframe_node_id].get_iframe_node() {
                        Some(iframe_node) => (iframe_node.callback.cb)(
                            &mut iframe_node.data,
                            &mut iframe_callback_info,
                        ),
                        None => IFrameCallbackReturn::default(),
                    }
                };

                let IFrameCallbackReturn {
                    dom,
                    scroll_size,
                    scroll_offset,
                    virtual_scroll_size,
                    virtual_scroll_offset,
                } = iframe_return;

                let mut iframe_dom = dom;
                let (scroll_node_id, scroll_dom_id) = match parent_dom_id {
                    Some(s) => (iframe_node_id, s),
                    None => (NodeId::ZERO, DomId { inner: 0 }),
                };

                // layout_result.scrollable_nodes.get();
                // parent_dom_id

                // TODO: use other fields of iframe_return here!

                let hovered_nodes = full_window_state
                    .last_hit_test
                    .hovered_nodes
                    .get(&iframe_dom_id)
                    .map(|i| i.regular_hit_test_nodes.clone())
                    .unwrap_or_default()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>();

                let active_nodes = if !full_window_state.mouse_state.mouse_down() {
                    Vec::new()
                } else {
                    hovered_nodes.clone()
                };

                let _ = iframe_dom.restyle_nodes_hover(hovered_nodes.as_slice(), true);
                let _ = iframe_dom.restyle_nodes_active(active_nodes.as_slice(), true);
                if let Some(focused_node) = full_window_state.focused_node {
                    if focused_node.dom == iframe_dom_id {
                        let _ = iframe_dom.restyle_nodes_focus(
                            &[focused_node.node.into_crate_internal().unwrap()],
                            true,
                        );
                    }
                }

                // TODO: use the iframe static position here?
                let bounds =
                    LogicalRect::new(LogicalPosition::zero(), hidpi_bounds.get_logical_size());

                // push the styled iframe dom into the next iframes and repeat (recurse)
                new_doms.push((Some(dom_id), iframe_dom_id, iframe_dom, bounds));
                new_scroll_states.push(NewIframeScrollState {
                    dom_id: scroll_dom_id,
                    node_id: scroll_node_id,
                    child_rect: LogicalRect {
                        origin: scroll_offset,
                        size: scroll_size,
                    },
                    virtual_child_rect: LogicalRect {
                        origin: virtual_scroll_offset,
                        size: virtual_scroll_size,
                    },
                });
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

    for nss in new_scroll_states {
        if let Some(lr) = resolved_doms.get_mut(nss.dom_id.inner) {
            let mut osn = lr
                .scrollable_nodes
                .overflowing_nodes
                .entry(NodeHierarchyItemId::from_crate_internal(Some(nss.node_id)))
                .or_insert_with(|| OverflowingScrollNode::default());

            osn.child_rect = nss.child_rect;
            osn.virtual_child_rect = nss.virtual_child_rect;
        }
    }

    if let Some(messages) = debug_messages.as_mut() {
        messages.push(LayoutDebugMessage {
            message: format!("Layout completed with {} DOMs", resolved_doms.len()).into(),
            location: "do_the_layout".to_string().into(),
        });
    }

    resolved_doms
}

/// At this point in time, all font keys, image keys, etc. have to be already
/// been submitted to the RenderApi and the AppResources!
#[cfg(feature = "text_layout")]
pub fn do_the_layout_internal(
    dom_id: DomId,
    parent_dom_id: Option<DomId>,
    mut styled_dom: StyledDom,
    renderer_resources: &mut RendererResources,
    document_id: &DocumentId,
    bounds: LogicalRect,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LayoutResult {
    use azul_core::app_resources::DecodedImage;

    use crate::text::{
        layout::{split_text_into_words_with_hyphenation, HyphenationCache},
        HYPHENATION_CACHE,
    };

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Layout internal for DOM {}", dom_id.inner).into(),
            location: "do_the_layout_internal".to_string().into(),
        });
    }

    let rect_size = bounds.size;
    let rect_offset = bounds.origin;

    let all_parents_btreeset = styled_dom
        .non_leaf_nodes
        .iter()
        .filter_map(|p| Some(p.node_id.into_crate_internal()?))
        .collect::<BTreeSet<_>>();

    let layout_position_info = get_layout_positions(&styled_dom);
    let layout_flex_grow_info = get_layout_flex_grows(&styled_dom);
    let layout_display_info = get_layout_displays(&styled_dom);
    let layout_directions_info = get_layout_flex_directions(&styled_dom);
    let layout_justify_contents = get_layout_justify_contents(&styled_dom);
    let layout_offsets = precalculate_all_offsets(&styled_dom);
    let layout_width_heights = precalculate_wh_config(&styled_dom);

    // Extract text layout options for word positioning
    let text_layout_options = extract_text_layout_options(&styled_dom);

    // Break all strings into words and / or resolve the TextIds
    let word_cache = create_word_cache_with_hyphenation(
        &styled_dom.node_data.as_container(),
        &text_layout_options,
        &HYPHENATION_CACHE,
        debug_messages,
    );

    // Scale the words to the correct size
    let shaped_words = create_shaped_words(renderer_resources, &word_cache, &styled_dom);

    let all_nodes_btreeset = (0..styled_dom.node_data.as_container().len())
        .map(|n| NodeId::new(n))
        .collect::<BTreeSet<_>>();

    // same as all_nodes_btreeset, but only for the words
    let display_none_nodes = get_display_none_nodes(
        &styled_dom.node_hierarchy.as_container(),
        &layout_display_info.as_ref(),
    );
    let all_word_nodes_btreeset = (0..styled_dom.node_data.as_container().len())
        .filter(|n| !display_none_nodes[*n]) // if the word block is marked as display:none, ignore
        .map(|n| NodeId::new(n)).collect::<BTreeSet<_>>();

    // Layout all words with the enhanced positioning function
    let mut word_positions_no_max_width = BTreeMap::new();
    create_word_positions(
        &mut word_positions_no_max_width,
        &all_word_nodes_btreeset,
        renderer_resources,
        &word_cache,
        &shaped_words,
        &styled_dom,
        None,
        &text_layout_options,
        debug_messages,
    );

    // Calculate the optional "intrinsic content widths" - i.e.
    // the width of a text or image, if no constraint would apply
    let mut content_widths_pre = styled_dom
        .node_data
        .as_container_mut()
        .transform_multithread(|node_data, node_id| {
            if display_none_nodes[node_id.index()] {
                None
            } else {
                match node_data.get_node_type() {
                    NodeType::Image(i) => match i.get_data() {
                        DecodedImage::NullImage { width, .. } => Some(*width as f32),
                        DecodedImage::Gl(tex) => Some(tex.size.width as f32),
                        DecodedImage::Raw((desc, _)) => Some(desc.width as f32),
                        _ => None,
                    },
                    _ => None,
                }
            }
        });

    for (node_id, word_positions) in word_positions_no_max_width.iter() {
        content_widths_pre.as_ref_mut()[*node_id] = Some(word_positions.0.content_size.width);
    }

    let mut width_calculated_arena = width_calculated_rect_arena_from_rect_layout_arena(
        &layout_width_heights.as_ref(),
        &layout_offsets.as_ref(),
        &content_widths_pre.as_ref(),
        &styled_dom.node_hierarchy.as_container(),
        &styled_dom.non_leaf_nodes.as_ref(),
        rect_size.width,
    );

    display_none_nodes
        .iter()
        .zip(width_calculated_arena.as_ref_mut().internal.iter_mut())
        .for_each(|(display_none, width)| {
            if *display_none {
                *width = WidthCalculatedRect::default();
            }
        });

    solve_flex_layout_width(
        &mut width_calculated_arena,
        &layout_flex_grow_info.as_ref(),
        &layout_display_info.as_ref(),
        &layout_position_info.as_ref(),
        &layout_directions_info.as_ref(),
        &styled_dom.node_hierarchy.as_container(),
        &layout_width_heights.as_ref(),
        styled_dom.non_leaf_nodes.as_ref(),
        rect_size.width,
        &all_parents_btreeset,
    );

    // If the flex grow / max-width step has caused the text block
    // to shrink in width, it needs to recalculate its height
    let word_blocks_to_recalculate = word_positions_no_max_width
        .iter()
        .filter_map(|(node_id, word_positions)| {
            let parent_id = styled_dom.node_hierarchy.as_container()[*node_id]
                .parent_id()
                .unwrap_or(NodeId::ZERO);
            if width_calculated_arena.as_ref()[*node_id].total()
                < word_positions.0.content_size.width
            {
                Some(*node_id)
            // if the text content overflows the parent width, we also need to recalculate
            } else if width_calculated_arena.as_ref()[*node_id].total()
                > width_calculated_arena.as_ref()[parent_id].total()
            {
                Some(*node_id)
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();

    // Recalculate the height of the content blocks for the word blocks that need it
    create_word_positions(
        &mut word_positions_no_max_width,
        &word_blocks_to_recalculate,
        renderer_resources,
        &word_cache,
        &shaped_words,
        &styled_dom,
        Some(&width_calculated_arena.as_ref()),
        &text_layout_options,
        debug_messages,
    );
    let word_positions_with_max_width = word_positions_no_max_width;

    // Calculate the content height of the (text / image) content based on its width
    let mut content_heights_pre = styled_dom
        .node_data
        .as_container_mut()
        .transform_multithread(|node_data, node_id| {
            let (raw_width, raw_height) = if display_none_nodes[node_id.index()] {
                None
            } else {
                match node_data.get_node_type() {
                    NodeType::Image(i) => match i.get_data() {
                        DecodedImage::NullImage { width, height, .. } => {
                            Some((*width as f32, *height as f32))
                        }
                        DecodedImage::Gl(tex) => {
                            Some((tex.size.width as f32, tex.size.height as f32))
                        }
                        DecodedImage::Raw((desc, _)) => {
                            Some((desc.width as f32, desc.height as f32))
                        }
                        _ => None,
                    },
                    _ => None,
                }
            }?;

            let current_width = width_calculated_arena.as_ref()[node_id].total();

            // preserve aspect ratio
            Some(raw_height / raw_width * current_width)
        });
    for (node_id, word_positions) in word_positions_with_max_width.iter() {
        content_heights_pre.as_ref_mut()[*node_id] = Some(word_positions.0.content_size.height);
    }

    // TODO: The content height is not the final height!
    let mut height_calculated_arena = height_calculated_rect_arena_from_rect_layout_arena(
        &layout_width_heights.as_ref(),
        &layout_offsets.as_ref(),
        &content_heights_pre.as_ref(),
        &styled_dom.node_hierarchy.as_container(),
        &styled_dom.non_leaf_nodes.as_ref(),
        rect_size.height,
    );

    display_none_nodes
        .iter()
        .zip(height_calculated_arena.as_ref_mut().internal.iter_mut())
        .for_each(|(display_none, height)| {
            if *display_none {
                *height = HeightCalculatedRect::default();
            }
        });

    solve_flex_layout_height(
        &mut height_calculated_arena,
        &layout_flex_grow_info.as_ref(),
        &layout_display_info.as_ref(),
        &layout_position_info.as_ref(),
        &layout_directions_info.as_ref(),
        &styled_dom.node_hierarchy.as_container(),
        &layout_width_heights.as_ref(),
        styled_dom.non_leaf_nodes.as_ref(),
        rect_size.height,
        &all_parents_btreeset,
    );

    let mut x_positions = NodeDataContainer {
        internal: vec![HorizontalSolvedPosition(0.0); styled_dom.node_data.len()].into(),
    };

    get_x_positions(
        &mut x_positions,
        &width_calculated_arena.as_ref(),
        &styled_dom.node_hierarchy.as_container(),
        &layout_position_info.as_ref(),
        &layout_directions_info.as_ref(),
        &layout_justify_contents.as_ref(),
        &styled_dom.non_leaf_nodes.as_ref(),
        rect_offset.clone(),
        &all_parents_btreeset,
    );

    let mut y_positions = NodeDataContainer {
        internal: vec![VerticalSolvedPosition(0.0); styled_dom.node_data.as_ref().len()].into(),
    };

    get_y_positions(
        &mut y_positions,
        &height_calculated_arena.as_ref(),
        &styled_dom.node_hierarchy.as_container(),
        &layout_position_info.as_ref(),
        &layout_directions_info.as_ref(),
        &layout_justify_contents.as_ref(),
        &styled_dom.non_leaf_nodes.as_ref(),
        rect_offset,
        &all_parents_btreeset,
    );

    let mut positioned_rects = NodeDataContainer {
        internal: vec![PositionedRectangle::default(); styled_dom.node_data.len()].into(),
    };
    let nodes_that_updated_positions = all_nodes_btreeset.clone();
    let nodes_that_need_to_redraw_text = all_nodes_btreeset.clone();

    position_nodes(
        &mut positioned_rects.as_ref_mut(),
        &styled_dom,
        AllOffsetsProvider::All(&layout_offsets.as_ref()),
        &width_calculated_arena.as_ref(),
        &height_calculated_arena.as_ref(),
        &x_positions.as_ref(),
        &y_positions.as_ref(),
        &nodes_that_updated_positions,
        &nodes_that_need_to_redraw_text,
        &layout_position_info.as_ref(),
        &word_cache,
        &shaped_words,
        &word_positions_with_max_width,
        document_id,
        debug_messages,
    );

    let mut overflowing_rects = ScrolledNodes::default();
    get_nodes_that_need_scroll_clip(
        &mut overflowing_rects,
        &styled_dom.styled_nodes.as_container(),
        &styled_dom.node_data.as_container(),
        &styled_dom.node_hierarchy.as_container(),
        &positioned_rects.as_ref(),
        styled_dom.non_leaf_nodes.as_ref(),
        dom_id,
        document_id,
    );

    let mut gpu_value_cache = GpuValueCache::empty();
    let _ = gpu_value_cache.synchronize(&positioned_rects.as_ref(), &styled_dom);

    // Process inline elements after basic layout is done
    process_inline_elements(
        &styled_dom,
        &mut width_calculated_arena,
        &mut height_calculated_arena,
        &x_positions.as_ref(),
        &y_positions.as_ref(),
        debug_messages,
    );

    // Return the final LayoutResult
    LayoutResult {
        dom_id,
        parent_dom_id,
        styled_dom,
        root_size: LayoutSize::new(
            rect_size.width.round() as isize,
            rect_size.height.round() as isize,
        ),
        root_position: LayoutPoint::new(
            rect_offset.x.round() as isize,
            rect_offset.y.round() as isize,
        ),
        preferred_widths: content_widths_pre,
        preferred_heights: content_heights_pre,
        width_calculated_rects: width_calculated_arena,
        height_calculated_rects: height_calculated_arena,
        solved_pos_x: x_positions,
        solved_pos_y: y_positions,
        layout_displays: layout_display_info,
        layout_flex_grows: layout_flex_grow_info,
        layout_positions: layout_position_info,
        layout_flex_directions: layout_directions_info,
        layout_justify_contents,
        rects: positioned_rects,
        words_cache: word_cache,
        shaped_words_cache: shaped_words,
        positioned_words_cache: word_positions_with_max_width,
        scrollable_nodes: overflowing_rects,
        iframe_mapping: BTreeMap::new(),
        gpu_value_cache,
    }
}

/// resets the preferred width / height to 0px before the layout is calculate
fn get_display_none_nodes<'a, 'b>(
    node_hierarchy: &'b NodeDataContainerRef<'a, NodeHierarchyItem>,
    layout_displays: &NodeDataContainerRef<'a, CssPropertyValue<LayoutDisplay>>,
) -> Vec<bool> {
    let mut items_that_should_be_set_to_zero = vec![false; layout_displays.internal.len()];
    if layout_displays.internal.is_empty() || layout_displays.internal.len() != node_hierarchy.len()
    {
        return items_that_should_be_set_to_zero;
    }

    let mut current = 0;
    while current < items_that_should_be_set_to_zero.len() {
        let display = layout_displays.internal[current].clone();

        if display == CssPropertyValue::None
            || display == CssPropertyValue::Exact(LayoutDisplay::None)
        {
            items_that_should_be_set_to_zero[current] = true;

            // set all children as display:none
            let subtree_len = node_hierarchy.subtree_len(NodeId::new(current));
            for child_id in current..=(current + subtree_len) {
                items_that_should_be_set_to_zero[child_id] = true;
            }

            current += subtree_len + 1;
        } else {
            current += 1;
        }
    }

    items_that_should_be_set_to_zero
}

/// Note: because this function is called both on layout() and relayout(),
/// the offsets are calculated during the layout() run. However,
/// we don't want to store all offsets because that would waste memory
///
/// So you can EITHER specify all offsets (useful during layout()) or specify
/// only the offsets of nodes that need to be recalculated (useful during relayout())
///
/// If an offset isn't found (usually shouldn't happen), the final positioned
/// rectangle is not positioned.
enum AllOffsetsProvider<'a> {
    All(&'a NodeDataContainerRef<'a, AllOffsets>),
    OnlyRecalculatedNodes(&'a BTreeMap<NodeId, AllOffsets>),
}

impl<'a> AllOffsetsProvider<'a> {
    fn get_offsets_for_node(&self, node_id: &NodeId) -> Option<&AllOffsets> {
        match self {
            AllOffsetsProvider::All(a) => Some(&a[*node_id]),
            AllOffsetsProvider::OnlyRecalculatedNodes(b) => b.get(node_id),
        }
    }
}

fn position_nodes<'a>(
    positioned_rects: &mut NodeDataContainerRefMut<'a, PositionedRectangle>,
    styled_dom: &StyledDom,
    offsets: AllOffsetsProvider<'a>,
    solved_widths: &NodeDataContainerRef<'a, WidthCalculatedRect>,
    solved_heights: &NodeDataContainerRef<'a, HeightCalculatedRect>,
    x_positions: &NodeDataContainerRef<'a, HorizontalSolvedPosition>,
    y_positions: &NodeDataContainerRef<'a, VerticalSolvedPosition>,
    nodes_that_updated_positions: &BTreeSet<NodeId>,
    nodes_that_need_to_redraw_text: &BTreeSet<NodeId>,
    position_info: &NodeDataContainerRef<'a, LayoutPosition>,
    word_cache: &BTreeMap<NodeId, Words>,
    shaped_words: &BTreeMap<NodeId, ShapedWords>,
    word_positions: &BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    document_id: &DocumentId,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) {
    use azul_core::ui_solver::PositionInfo;

    let mut positioned_node_stack = vec![NodeId::new(0)];
    let css_property_cache = styled_dom.get_css_property_cache();
    let styled_nodes = styled_dom.styled_nodes.as_container();
    let node_hierarchy = &styled_dom.node_hierarchy.as_container();

    // create the final positioned rectangles
    for ParentWithNodeDepth { depth: _, node_id } in styled_dom.non_leaf_nodes.as_ref().iter() {
        let parent_node_id = match node_id.into_crate_internal() {
            Some(s) => s,
            None => continue,
        };
        if !nodes_that_updated_positions.contains(&parent_node_id) {
            continue;
        };

        let parent_position = position_info[parent_node_id];
        let width = solved_widths[parent_node_id];
        let height = solved_heights[parent_node_id];
        let x_pos = x_positions[parent_node_id].0;
        let y_pos = y_positions[parent_node_id].0;

        let parent_parent_node_id = node_hierarchy[parent_node_id]
            .parent_id()
            .unwrap_or(NodeId::new(0));
        let parent_x_pos = x_positions[parent_parent_node_id].0;
        let parent_y_pos = y_positions[parent_parent_node_id].0;
        let parent_parent_width = solved_widths[parent_parent_node_id];
        let parent_parent_height = solved_heights[parent_parent_node_id];

        let last_positioned_item_node_id = positioned_node_stack
            .last()
            .map(|l| *l)
            .unwrap_or(NodeId::new(0));
        let last_positioned_item_x_pos = x_positions[last_positioned_item_node_id].0;
        let last_positioned_item_y_pos = y_positions[last_positioned_item_node_id].0;
        let parent_position_info = match parent_position {
            LayoutPosition::Static => PositionInfo::Static(PositionInfoInner {
                // calculate relative to parent
                x_offset: x_pos - parent_x_pos,
                y_offset: y_pos - parent_y_pos,
                static_x_offset: x_pos,
                static_y_offset: y_pos,
            }),
            LayoutPosition::Relative => PositionInfo::Relative(PositionInfoInner {
                // calculate relative to parent
                x_offset: x_pos - parent_x_pos,
                y_offset: y_pos - parent_y_pos,
                static_x_offset: x_pos,
                static_y_offset: y_pos,
            }),
            LayoutPosition::Absolute => PositionInfo::Absolute(PositionInfoInner {
                // calculate relative to last positioned item
                x_offset: x_pos - last_positioned_item_x_pos,
                y_offset: y_pos - last_positioned_item_y_pos,
                static_x_offset: x_pos,
                static_y_offset: y_pos,
            }),
            LayoutPosition::Fixed => PositionInfo::Fixed(PositionInfoInner {
                // relative to screen, already done
                x_offset: x_pos,
                y_offset: y_pos,
                static_x_offset: x_pos,
                static_y_offset: y_pos,
            }),
        };
        let parent_size = LogicalSize::new(width.total(), height.total());

        let parent_offsets = match offsets.get_offsets_for_node(&parent_node_id) {
            Some(s) => s,
            None => continue,
        };

        let parent_padding = parent_offsets
            .padding
            .resolve(parent_parent_width.total(), parent_parent_height.total());
        let parent_margin = parent_offsets
            .margin
            .resolve(parent_parent_width.total(), parent_parent_height.total());
        let parent_border_widths = parent_offsets
            .border_widths
            .resolve(parent_parent_width.total(), parent_parent_height.total());

        // push positioned item and layout children
        if parent_position != LayoutPosition::Static {
            positioned_node_stack.push(parent_node_id);
        }

        for child_node_id in parent_node_id.az_children(node_hierarchy) {
            // copy the width and height from the parent node
            let parent_width = width;
            let parent_height = height;
            let parent_x_pos = x_pos;
            let parent_y_pos = y_pos;

            let width = solved_widths[child_node_id];
            let height = solved_heights[child_node_id];
            let x_pos = x_positions[child_node_id].0;
            let y_pos = y_positions[child_node_id].0;
            let child_position = position_info[child_node_id];
            let child_styled_node_state = &styled_nodes[child_node_id].state;
            let child_node_data = &styled_dom.node_data.as_container()[child_node_id];

            let child_position = match child_position {
                LayoutPosition::Static => PositionInfo::Static(PositionInfoInner {
                    // calculate relative to parent
                    x_offset: x_pos - parent_x_pos,
                    y_offset: y_pos - parent_y_pos,
                    static_x_offset: x_pos,
                    static_y_offset: y_pos,
                }),
                LayoutPosition::Relative => PositionInfo::Relative(PositionInfoInner {
                    // calculate relative to parent
                    x_offset: x_pos - parent_x_pos,
                    y_offset: y_pos - parent_y_pos,
                    static_x_offset: x_pos,
                    static_y_offset: y_pos,
                }),
                LayoutPosition::Absolute => PositionInfo::Absolute(PositionInfoInner {
                    // calculate relative to last positioned item
                    x_offset: x_pos - last_positioned_item_x_pos,
                    y_offset: y_pos - last_positioned_item_y_pos,
                    static_x_offset: x_pos,
                    static_y_offset: y_pos,
                }),
                LayoutPosition::Fixed => PositionInfo::Fixed(PositionInfoInner {
                    // relative to screen, already done
                    x_offset: x_pos,
                    y_offset: y_pos,
                    static_x_offset: x_pos,
                    static_y_offset: y_pos,
                }),
            };

            let child_size_logical = LogicalSize::new(width.total(), height.total());
            let child_offsets = match offsets.get_offsets_for_node(&child_node_id) {
                Some(s) => s,
                None => continue,
            };

            let child_padding = child_offsets
                .padding
                .resolve(parent_width.total(), parent_height.total());
            let child_margin = child_offsets
                .margin
                .resolve(parent_width.total(), parent_height.total());
            let child_border_widths = child_offsets
                .border_widths
                .resolve(parent_width.total(), parent_height.total());

            // set text, if any
            let child_text = if let (Some(words), Some(shaped_words), Some((word_positions, _))) = (
                word_cache.get(&child_node_id),
                shaped_words.get(&child_node_id),
                word_positions.get(&child_node_id),
            ) {
                if nodes_that_need_to_redraw_text.contains(&child_node_id) {
                    #[cfg(feature = "text_layout")]
                    {
                        use crate::text::InlineText;

                        let mut inline_text_layout = InlineText {
                            words,
                            shaped_words,
                        }
                        .get_text_layout(
                            document_id,
                            child_node_id,
                            &word_positions.text_layout_options,
                        );

                        let (horz_alignment, vert_alignment) = determine_text_alignment(
                            css_property_cache
                                .get_align_items(
                                    child_node_data,
                                    &child_node_id,
                                    child_styled_node_state,
                                )
                                .cloned()
                                .and_then(|p| p.get_property_or_default())
                                .unwrap_or_default(),
                            css_property_cache
                                .get_justify_content(
                                    child_node_data,
                                    &child_node_id,
                                    child_styled_node_state,
                                )
                                .cloned()
                                .and_then(|p| p.get_property_or_default())
                                .unwrap_or_default(),
                            css_property_cache
                                .get_text_align(
                                    child_node_data,
                                    &child_node_id,
                                    child_styled_node_state,
                                )
                                .cloned(),
                        );

                        inline_text_layout
                            .align_children_horizontal(&child_size_logical, horz_alignment);
                        inline_text_layout.align_children_vertical_in_parent_bounds(
                            &child_size_logical,
                            vert_alignment,
                        );

                        Some((
                            word_positions.text_layout_options.clone(),
                            inline_text_layout,
                        ))
                    }
                    #[cfg(not(feature = "text_layout"))]
                    {
                        None
                    }
                } else {
                    positioned_rects[child_node_id]
                        .resolved_text_layout_options
                        .clone()
                }
            } else {
                None
            };

            let positioned_rect = PositionedRectangle {
                size: LogicalSize::new(width.total(), height.total()),
                position: child_position,
                padding: child_padding,
                margin: child_margin,
                box_shadow: child_offsets.box_shadow,
                box_sizing: child_offsets.box_sizing,
                border_widths: child_border_widths,
                resolved_text_layout_options: child_text,
                overflow_x: child_offsets.overflow_x,
                overflow_y: child_offsets.overflow_y,
            };

            positioned_rects[child_node_id] = positioned_rect;
        }

        // NOTE: Intentionally do not set text_layout_options,
        // otherwise this would overwrite the existing text layout options
        // Label / Text nodes are ALWAYS children of some parent node,
        // they can not be root nodes. Therefore the children_iter() will take
        // care of layouting the text
        let parent_rect = &mut positioned_rects[parent_node_id];
        parent_rect.size = parent_size;
        parent_rect.position = parent_position_info;
        parent_rect.padding = parent_padding;
        parent_rect.margin = parent_margin;
        parent_rect.border_widths = parent_border_widths;
        parent_rect.box_shadow = parent_offsets.box_shadow;
        parent_rect.box_sizing = parent_offsets.box_sizing;
        parent_rect.overflow_x = parent_offsets.overflow_x;
        parent_rect.overflow_y = parent_offsets.overflow_y;

        if parent_position != LayoutPosition::Static {
            positioned_node_stack.pop();
        }
    }
}

// Create word cache with hyphenation
#[cfg(feature = "text_layout")]
fn create_word_cache_with_hyphenation<'a>(
    node_data: &NodeDataContainerRef<'a, NodeData>,
    text_layout_options: &BTreeMap<NodeId, ResolvedTextLayoutOptions>,
    hyphenation_cache: &crate::text::layout::HyphenationCache,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> BTreeMap<NodeId, Words> {
    use crate::text::layout::split_text_into_words_with_hyphenation;

    node_data
        .internal
        .iter()
        .enumerate()
        .filter_map(|(node_id, node)| {
            let node_id = NodeId::new(node_id);
            let string = match node.get_node_type() {
                NodeType::Text(string) => Some(string.as_str()),
                _ => None,
            }?;

            // Get layout options for this node or use default
            let options = text_layout_options
                .get(&node_id)
                .cloned()
                .unwrap_or_default();

            // Use enhanced word splitting with hyphenation
            Some((
                node_id,
                split_text_into_words_with_hyphenation(
                    string,
                    &options,
                    hyphenation_cache,
                    debug_messages,
                ),
            ))
        })
        .collect()
}

#[cfg(feature = "text_layout")]
fn create_word_cache<'a>(
    node_data: &NodeDataContainerRef<'a, NodeData>,
) -> BTreeMap<NodeId, Words> {
    use crate::text::layout::split_text_into_words;

    let word_map = node_data
        .internal
        .iter()
        .enumerate()
        .map(|(node_id, node)| {
            let node_id = NodeId::new(node_id);
            let string = match node.get_node_type() {
                NodeType::Text(string) => Some(string.as_str()),
                _ => None,
            }?;
            Some((node_id, split_text_into_words(string)))
        })
        .collect::<Vec<_>>();

    word_map.into_iter().filter_map(|a| a).collect()
}

// same as get_inline_text(), but shapes a new word instead of using the internal one
// - necessary to implement text cursor, so that we can calculate the x-offset of
// the text cursor for the next frame (after the character has been pressed)
#[cfg(feature = "text_layout")]
pub fn callback_info_shape_text(
    callbackinfo: &CallbackInfo,
    node_id: DomNodeId,
    text: AzString,
) -> Option<InlineText> {
    let font_ref = callbackinfo.get_font_ref(node_id)?;
    let text_layout_options = callbackinfo.get_text_layout_options(node_id)?;
    Some(crate::text::layout::shape_text(
        &font_ref,
        text.as_str(),
        &text_layout_options,
    ))
}

#[cfg(feature = "text_layout")]
pub fn create_shaped_words<'a>(
    renderer_resources: &RendererResources,
    words: &BTreeMap<NodeId, Words>,
    styled_dom: &'a StyledDom,
) -> BTreeMap<NodeId, ShapedWords> {
    use crate::text::{layout::shape_words, shaping::ParsedFont};

    let css_property_cache = styled_dom.get_css_property_cache();
    let styled_nodes = styled_dom.styled_nodes.as_container();
    let node_data = styled_dom.node_data.as_container();

    words
        .iter()
        .filter_map(|(node_id, words)| {
            use azul_core::styled_dom::StyleFontFamiliesHash;

            let styled_node_state = &styled_nodes[*node_id].state;
            let node_data = &node_data[*node_id];
            let css_font_families =
                css_property_cache.get_font_id_or_default(node_data, node_id, styled_node_state);
            let css_font_families_hash = StyleFontFamiliesHash::new(css_font_families.as_ref());
            let css_font_family = renderer_resources.get_font_family(&css_font_families_hash)?;
            let font_key = renderer_resources.get_font_key(&css_font_family)?;
            let (font_ref, _) = renderer_resources.get_registered_font(&font_key)?;
            let font_data = font_ref.get_data();

            // downcast the loaded_font.font from *const c_void to *const ParsedFont
            let parsed_font_downcasted = unsafe { &*(font_data.parsed as *const ParsedFont) };

            let shaped_words = shape_words(words, parsed_font_downcasted);

            Some((*node_id, shaped_words))
        })
        .collect()
}

#[cfg(feature = "text_layout")]
fn create_word_positions<'a>(
    word_positions: &mut BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    word_positions_to_generate: &BTreeSet<NodeId>,
    renderer_resources: &RendererResources,
    words: &BTreeMap<NodeId, Words>,
    shaped_words: &BTreeMap<NodeId, ShapedWords>,
    styled_dom: &'a StyledDom,
    solved_widths: Option<&'a NodeDataContainerRef<'a, WidthCalculatedRect>>,
    text_layout_options: &BTreeMap<NodeId, ResolvedTextLayoutOptions>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) {
    use azul_core::{
        app_resources::font_size_to_au,
        ui_solver::{DEFAULT_LETTER_SPACING, DEFAULT_WORD_SPACING},
    };

    use crate::text::layout::position_words;

    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();

    let collected = words
        .iter()
        .filter_map(|(node_id, words)| {
            use azul_core::styled_dom::StyleFontFamiliesHash;

            if !word_positions_to_generate.contains(node_id) {
                return None;
            }
            let node_data = &node_data_container[*node_id];

            let styled_node_state = &styled_dom.styled_nodes.as_container()[*node_id].state;
            let font_size =
                css_property_cache.get_font_size_or_default(node_data, node_id, &styled_node_state);
            let font_size_au = font_size_to_au(font_size);
            let font_size_px = font_size.inner.to_pixels(DEFAULT_FONT_SIZE_PX as f32);

            let css_font_families =
                css_property_cache.get_font_id_or_default(node_data, node_id, styled_node_state);
            let css_font_families_hash = StyleFontFamiliesHash::new(css_font_families.as_ref());
            let css_font_family = renderer_resources.get_font_family(&css_font_families_hash)?;
            let font_key = renderer_resources.get_font_key(&css_font_family)?;
            let (_, font_instances) = renderer_resources.get_registered_font(&font_key)?;

            let font_instance_key = font_instances
                .iter()
                .find(|(k, v)| k.0 == font_size_au)
                .map(|(_, v)| v)?;

            let shaped_words = shaped_words.get(&node_id)?;

            let mut max_text_width = None;
            let mut cur_node = *node_id;
            while let Some(parent) = styled_dom.node_hierarchy.as_container()[*node_id].parent_id()
            {
                let overflow_x = css_property_cache
                    .get_overflow_x(
                        &node_data_container[parent],
                        &parent,
                        &styled_dom.styled_nodes.as_container()[parent].state,
                    )
                    .cloned();

                match overflow_x {
                    Some(CssPropertyValue::Exact(LayoutOverflow::Hidden))
                    | Some(CssPropertyValue::Exact(LayoutOverflow::Visible)) => {
                        max_text_width = None;
                        break;
                    }
                    None
                    | Some(CssPropertyValue::None)
                    | Some(CssPropertyValue::Auto)
                    | Some(CssPropertyValue::Initial)
                    | Some(CssPropertyValue::Inherit)
                    | Some(CssPropertyValue::Exact(LayoutOverflow::Auto))
                    | Some(CssPropertyValue::Exact(LayoutOverflow::Scroll)) => {
                        max_text_width = solved_widths.map(|sw| sw[parent].total() as f32);
                        break;
                    }
                }

                cur_node = parent;
            }

            // Get the specific text layout options for this node or use default
            let mut options = text_layout_options
                .get(node_id)
                .cloned()
                .unwrap_or_default();

            // Update the max_width based on the parent container
            options.max_horizontal_width = max_text_width.into();

            // Use the enhanced word positioning function
            let w = position_words(words, shaped_words, &options, debug_messages);

            Some((*node_id, (w, *font_instance_key)))
        })
        .collect::<Vec<_>>();

    collected.into_iter().for_each(|(node_id, word_position)| {
        word_positions.insert(node_id, word_position);
    });
}

/// For a given rectangle, determines what text alignment should be used
fn determine_text_alignment(
    align_items: LayoutAlignItems,
    justify_content: LayoutJustifyContent,
    text_align: Option<CssPropertyValue<StyleTextAlign>>,
) -> (StyleTextAlign, StyleVerticalAlign) {
    // Vertical text alignment
    let vert_alignment = match align_items {
        LayoutAlignItems::FlexStart => StyleVerticalAlign::Top,
        LayoutAlignItems::FlexEnd => StyleVerticalAlign::Bottom,
        // technically stretch = blocktext, but we don't have that yet
        _ => StyleVerticalAlign::Center,
    };

    // Horizontal text alignment
    let mut horz_alignment = match justify_content {
        LayoutJustifyContent::Start => StyleTextAlign::Left,
        LayoutJustifyContent::End => StyleTextAlign::Right,
        _ => StyleTextAlign::Center,
    };

    if let Some(text_align) = text_align
        .as_ref()
        .and_then(|ta| ta.get_property().copied())
    {
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
    scrolled_nodes: &mut ScrolledNodes,
    display_list_rects: &NodeDataContainerRef<StyledNode>,
    dom_rects: &NodeDataContainerRef<NodeData>,
    node_hierarchy: &NodeDataContainerRef<NodeHierarchyItem>,
    layouted_rects: &NodeDataContainerRef<PositionedRectangle>,
    parents: &[ParentWithNodeDepth],
    dom_id: DomId,
    document_id: &DocumentId,
) {
    use azul_core::{
        dom::{ScrollTagId, TagId},
        styled_dom::NodeHierarchyItemId,
        ui_solver::{ExternalScrollId, OverflowingScrollNode},
    };

    let mut overflowing_nodes = BTreeMap::new();
    let mut tags_to_node_ids = BTreeMap::new();
    let mut clip_nodes = BTreeMap::new();

    // brute force: calculate all immediate children sum rects of all parents
    let mut all_direct_overflows = parents
        .iter()
        .filter_map(|ParentWithNodeDepth { depth: _, node_id }| {
            let parent_id = node_id.into_crate_internal()?;
            let parent_rect = layouted_rects[parent_id].get_approximate_static_bounds();

            // cannot create scroll clips if the parent is less than 1px wide
            if parent_rect.width() < 1 {
                return None;
            }

            let children_sum_rect = LayoutRect::union(
                parent_id
                    .az_children(node_hierarchy)
                    .filter(|child_id| {
                        use azul_core::ui_solver::PositionInfo;
                        match layouted_rects[*child_id].position {
                            PositionInfo::Absolute(_) => false,
                            _ => true,
                        }
                    })
                    .map(|child_id| layouted_rects[child_id].get_approximate_static_bounds()),
            )?;

            // only register the directly overflowing children
            if parent_rect.contains_rect(&children_sum_rect) {
                None
            } else {
                Some((parent_id, (parent_rect, children_sum_rect)))
            }
        })
        .collect::<BTreeMap<_, _>>();

    // Go from the inside out and bubble the overflowing rectangles
    // based on the overflow-x / overflow-y property
    let mut len = parents.len();
    while len != 0 {
        use azul_css::LayoutOverflow::*;

        len -= 1;

        let parent = &parents[len];
        let parent_id = match parent.node_id.into_crate_internal() {
            Some(s) => s,
            None => continue,
        };

        let (parent_rect, children_sum_rect) = match all_direct_overflows.get(&parent_id).cloned() {
            Some(s) => s,
            None => continue,
        };

        let positioned_rect = &layouted_rects[parent_id];
        let overflow_x = positioned_rect.overflow_x;
        let overflow_y = positioned_rect.overflow_y;

        match (overflow_x, overflow_y) {
            (Hidden, Hidden) => {
                clip_nodes.insert(parent_id, positioned_rect.size);
                all_direct_overflows.remove(&parent_id);
            }
            (Visible, Visible) => {
                all_direct_overflows.remove(&parent_id);
            }
            _ => {
                // modify the rect in the all_direct_overflows,
                // then recalculate the rectangles for all parents
                // this is expensive, but at least correct
            }
        }
    }

    // Insert all rectangles that need to scroll
    for (parent_id, (parent_rect, children_sum_rect)) in all_direct_overflows {
        use azul_core::callbacks::PipelineId;

        let parent_dom_hash = dom_rects[parent_id].calculate_node_data_hash();
        let parent_external_scroll_id = ExternalScrollId(
            parent_dom_hash.0,
            PipelineId(dom_id.inner as u32, document_id.id),
        );

        let scroll_tag_id = match display_list_rects[parent_id].tag_id.as_ref() {
            Some(s) => ScrollTagId(s.into_crate_internal()),
            None => ScrollTagId(TagId::unique()),
        };

        let child_rect = LogicalRect::new(
            LogicalPosition::new(
                children_sum_rect.origin.x as f32,
                children_sum_rect.origin.y as f32,
            ),
            LogicalSize::new(
                children_sum_rect.size.width as f32,
                children_sum_rect.size.height as f32,
            ),
        );

        let os = OverflowingScrollNode {
            parent_rect: LogicalRect::new(
                LogicalPosition::new(parent_rect.origin.x as f32, parent_rect.origin.y as f32),
                LogicalSize::new(
                    parent_rect.size.width as f32,
                    parent_rect.size.height as f32,
                ),
            ),
            child_rect,
            virtual_child_rect: child_rect,
            parent_external_scroll_id,
            parent_dom_hash,
            scroll_tag_id,
        };

        overflowing_nodes.insert(
            NodeHierarchyItemId::from_crate_internal(Some(parent_id)),
            os,
        );
        tags_to_node_ids.insert(
            scroll_tag_id,
            NodeHierarchyItemId::from_crate_internal(Some(parent_id)),
        );
    }

    *scrolled_nodes = ScrolledNodes {
        overflowing_nodes,
        clip_nodes,
        tags_to_node_ids,
    };
}

/// Relayout function, takes an existing LayoutResult and adjusts it
/// so that only the nodes that need relayout are touched.
/// See `CallbacksToCall`
///
/// Returns a vec of node IDs that whose layout was changed
pub fn do_the_relayout(
    dom_id: DomId,
    root_bounds: LayoutRect,
    layout_result: &mut LayoutResult,
    _image_cache: &ImageCache,
    renderer_resources: &mut RendererResources,
    document_id: &DocumentId,
    nodes_to_relayout: Option<&BTreeMap<NodeId, Vec<ChangedCssProperty>>>,
    words_to_relayout: Option<&BTreeMap<NodeId, AzString>>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> RelayoutChanges {
    // shortcut: in most cases, the root size hasn't
    // changed and there are no nodes to relayout

    let root_size = root_bounds.size;
    let root_size_changed = root_bounds != layout_result.get_bounds();

    if !root_size_changed && nodes_to_relayout.is_none() && words_to_relayout.is_none() {
        return RelayoutChanges::empty();
    }

    // merge the nodes to relayout by type so that we don't relayout twice
    let mut nodes_to_relayout = nodes_to_relayout.map(|n| {
        n.iter()
            .filter_map(|(node_id, changed_properties)| {
                let mut properties = BTreeMap::new();

                for prop in changed_properties.iter() {
                    let prop_type = prop.previous_prop.get_type();
                    if prop_type.can_trigger_relayout() {
                        properties.insert(prop_type, prop.clone());
                    }
                }

                if properties.is_empty() {
                    None
                } else {
                    Some((*node_id, properties))
                }
            })
            .collect::<BTreeMap<NodeId, BTreeMap<CssPropertyType, ChangedCssProperty>>>()
    });

    if !root_size_changed && nodes_to_relayout.is_none() && words_to_relayout.is_none() {
        let resized_nodes = Vec::new();
        let gpu_key_changes = layout_result
            .gpu_value_cache
            .synchronize(&layout_result.rects.as_ref(), &layout_result.styled_dom);

        return RelayoutChanges {
            resized_nodes,
            gpu_key_changes,
        };
    }

    // ---- step 1: recalc size

    // TODO: for now, the preferred_widths and preferred_widths is always None,
    // so the content width + height isn't taken into account. If that changes,
    // the new content size has to be calculated first!

    // TODO: changes to display, float and box-sizing property are ignored
    // TODO: changes to top, bottom, right, left property are ignored for now
    // TODO: changes to position: property are updated, but ignored for now

    // recalc(&mut layout_result.preferred_widths);

    let mut display_changed = false;

    // update the precalculated properties (position, flex-grow,
    // flex-direction, justify-content)
    if let Some(nodes_to_relayout) = nodes_to_relayout.as_ref() {
        nodes_to_relayout
            .iter()
            .for_each(|(node_id, changed_props)| {
                if let Some(CssProperty::Display(new_display_state)) = changed_props
                    .get(&CssPropertyType::Display)
                    .map(|p| &p.current_prop)
                {
                    display_changed = true;
                    layout_result.layout_displays.as_ref_mut()[*node_id] =
                        new_display_state.clone();
                }

                if let Some(CssProperty::Position(new_position_state)) = changed_props
                    .get(&CssPropertyType::Position)
                    .map(|p| &p.current_prop)
                {
                    layout_result.layout_positions.as_ref_mut()[*node_id] = new_position_state
                        .get_property()
                        .cloned()
                        .unwrap_or_default();
                }

                if let Some(CssProperty::FlexGrow(new_flex_grow)) = changed_props
                    .get(&CssPropertyType::FlexGrow)
                    .map(|p| &p.current_prop)
                {
                    layout_result.layout_flex_grows.as_ref_mut()[*node_id] = new_flex_grow
                        .get_property()
                        .cloned()
                        .map(|grow| grow.inner.get().max(0.0))
                        .unwrap_or(DEFAULT_FLEX_GROW_FACTOR);
                }

                if let Some(CssProperty::FlexDirection(new_flex_direction)) = changed_props
                    .get(&CssPropertyType::FlexDirection)
                    .map(|p| &p.current_prop)
                {
                    layout_result.layout_flex_directions.as_ref_mut()[*node_id] =
                        new_flex_direction
                            .get_property()
                            .cloned()
                            .unwrap_or_default();
                }

                if let Some(CssProperty::JustifyContent(new_justify_content)) = changed_props
                    .get(&CssPropertyType::JustifyContent)
                    .map(|p| &p.current_prop)
                {
                    layout_result.layout_justify_contents.as_ref_mut()[*node_id] =
                        new_justify_content
                            .get_property()
                            .cloned()
                            .unwrap_or_default();
                }
            });
    }

    let mut nodes_that_need_to_bubble_width = BTreeMap::new();
    let mut nodes_that_need_to_bubble_height = BTreeMap::new();
    let mut parents_that_need_to_recalc_width_of_children = BTreeSet::new();
    let mut parents_that_need_to_recalc_height_of_children = BTreeSet::new();
    let mut parents_that_need_to_reposition_children_x = BTreeSet::new();
    let mut parents_that_need_to_reposition_children_y = BTreeSet::new();

    /*
    if display_changed {
        // recalculate changed display:none nodes
        let new_display_none_nodes = get_display_none_nodes(
            &styled_dom.node_hierarchy.as_container(),
            &layout_result.layout_displays.as_ref(),
        );
    }
    */

    if root_size_changed {
        let all_parents_btreeset = layout_result
            .styled_dom
            .non_leaf_nodes
            .iter()
            .filter_map(|p| Some(p.node_id.into_crate_internal()?))
            .collect::<BTreeSet<_>>();
        layout_result.root_size = root_size;
        parents_that_need_to_recalc_width_of_children
            .extend(all_parents_btreeset.clone().into_iter());
        parents_that_need_to_recalc_height_of_children.extend(all_parents_btreeset.into_iter());
    }

    if root_size.width != layout_result.root_size.width {
        let root_id = layout_result.styled_dom.root.into_crate_internal().unwrap();
        parents_that_need_to_recalc_width_of_children.insert(root_id);
    }

    if root_size.height != layout_result.root_size.height {
        let root_id = layout_result.styled_dom.root.into_crate_internal().unwrap();
        parents_that_need_to_recalc_height_of_children.insert(root_id);
    }

    let mut node_ids_that_changed_text_content = BTreeSet::new();

    // Update words cache and shaped words cache
    #[cfg(feature = "text_layout")]
    if let Some(words_to_relayout) = words_to_relayout {
        for (node_id, new_string) in words_to_relayout.iter() {
            use azul_core::{
                styled_dom::StyleFontFamiliesHash,
                ui_solver::{DEFAULT_LETTER_SPACING, DEFAULT_WORD_SPACING},
            };

            use crate::text::{
                layout::{
                    position_words, shape_words, split_text_into_words,
                    word_positions_to_inline_text_layout,
                },
                shaping::ParsedFont,
            };

            if layout_result.words_cache.get(&node_id).is_none() {
                continue;
            }
            if layout_result.shaped_words_cache.get(&node_id).is_none() {
                continue;
            }
            if layout_result.positioned_words_cache.get(&node_id).is_none() {
                continue;
            }
            let text_layout_options = match layout_result
                .rects
                .as_ref()
                .get(*node_id)
                .and_then(|s| s.resolved_text_layout_options.as_ref())
            {
                None => continue,
                Some(s) => s.0.clone(),
            };

            let new_words = split_text_into_words(new_string.as_str());

            let css_property_cache = layout_result.styled_dom.get_css_property_cache();
            let styled_nodes = layout_result.styled_dom.styled_nodes.as_container();
            let node_data = layout_result.styled_dom.node_data.as_container();
            let styled_node_state = &styled_nodes[*node_id].state;
            let node_data = &node_data[*node_id];

            let css_font_families =
                css_property_cache.get_font_id_or_default(node_data, node_id, styled_node_state);
            let css_font_families_hash = StyleFontFamiliesHash::new(css_font_families.as_ref());
            let css_font_family = match renderer_resources.get_font_family(&css_font_families_hash)
            {
                Some(s) => s,
                None => continue,
            };
            let font_key = match renderer_resources.get_font_key(&css_font_family) {
                Some(s) => s,
                None => continue,
            };
            let (font_ref, _) = match renderer_resources.get_registered_font(&font_key) {
                Some(s) => s,
                None => continue,
            };
            let font_data = font_ref.get_data();
            let parsed_font_downcasted = unsafe { &*(font_data.parsed as *const ParsedFont) };
            let new_shaped_words = shape_words(&new_words, parsed_font_downcasted);

            let font_size =
                css_property_cache.get_font_size_or_default(node_data, node_id, &styled_node_state);
            let font_size_px = font_size.inner.to_pixels(DEFAULT_FONT_SIZE_PX as f32);

            let letter_spacing = css_property_cache
                .get_letter_spacing(node_data, node_id, &styled_node_state)
                .and_then(|ls| Some(ls.get_property()?.inner.to_pixels(DEFAULT_LETTER_SPACING)));

            let word_spacing = css_property_cache
                .get_word_spacing(node_data, node_id, &styled_node_state)
                .and_then(|ws| Some(ws.get_property()?.inner.to_pixels(DEFAULT_WORD_SPACING)));

            let line_height = css_property_cache
                .get_line_height(node_data, node_id, &styled_node_state)
                .and_then(|lh| Some(lh.get_property()?.inner.get()));

            let tab_width = css_property_cache
                .get_tab_width(node_data, node_id, &styled_node_state)
                .and_then(|tw| Some(tw.get_property()?.inner.get()));

            let new_word_positions = position_words(
                &new_words,
                &new_shaped_words,
                &text_layout_options,
                &mut None,
            );
            let new_inline_text_layout = word_positions_to_inline_text_layout(&new_word_positions);

            let old_word_dimensions = layout_result
                .rects
                .as_ref()
                .get(*node_id)
                .and_then(|s| s.resolved_text_layout_options.as_ref())
                .map(|s| s.1.content_size)
                .unwrap_or(LogicalSize::zero());

            let new_word_dimensions = new_word_positions.content_size;

            layout_result.preferred_widths.as_ref_mut()[*node_id] =
                Some(new_word_positions.content_size.width);
            *layout_result.words_cache.get_mut(node_id).unwrap() = new_words;
            *layout_result.shaped_words_cache.get_mut(node_id).unwrap() = new_shaped_words;
            layout_result
                .positioned_words_cache
                .get_mut(node_id)
                .unwrap()
                .0 = new_word_positions;
            layout_result
                .rects
                .as_ref_mut()
                .get_mut(*node_id)
                .unwrap()
                .resolved_text_layout_options = Some((text_layout_options, new_inline_text_layout));
            node_ids_that_changed_text_content.insert(*node_id);
        }
    }

    // parents need to be adjust before children
    for ParentWithNodeDepth { depth: _, node_id } in layout_result.styled_dom.non_leaf_nodes.iter()
    {
        macro_rules! detect_changes {
            ($node_id:expr, $parent_id:expr) => {
                let node_data = &layout_result.styled_dom.node_data.as_container()[$node_id];
                let text_content_has_changed =
                    node_ids_that_changed_text_content.contains(&$node_id);
                let default_changes = BTreeMap::new();
                let changes_for_this_node =
                    match nodes_to_relayout.as_ref().and_then(|n| n.get(&$node_id)) {
                        Some(s) => Some(s),
                        None => {
                            if text_content_has_changed {
                                Some(&default_changes)
                            } else {
                                None
                            }
                        }
                    };

                let has_word_positions = layout_result
                    .positioned_words_cache
                    .get(&$node_id)
                    .is_some();
                if let Some(changes_for_this_node) = changes_for_this_node.as_ref() {
                    if !changes_for_this_node.is_empty()
                        || has_word_positions
                        || text_content_has_changed
                    {
                        let mut preferred_width_changed = None;
                        let mut preferred_height_changed = None;
                        let mut padding_x_changed = false;
                        let mut padding_y_changed = false;
                        let mut margin_x_changed = false;
                        let mut margin_y_changed = false;

                        let solved_width_layout =
                            &mut layout_result.width_calculated_rects.as_ref_mut()[$node_id];
                        let solved_height_layout =
                            &mut layout_result.height_calculated_rects.as_ref_mut()[$node_id];
                        let css_property_cache = layout_result.styled_dom.get_css_property_cache();
                        let parent_parent = layout_result.styled_dom.node_hierarchy.as_container()
                            [$parent_id]
                            .parent_id()
                            .unwrap_or(NodeId::ZERO);

                        // recalculate min / max / preferred width constraint if needed
                        if changes_for_this_node.contains_key(&CssPropertyType::Width)
                            || changes_for_this_node.contains_key(&CssPropertyType::MinWidth)
                            || changes_for_this_node.contains_key(&CssPropertyType::MaxWidth)
                            || has_word_positions
                            || text_content_has_changed
                        {
                            let styled_node_state =
                                &layout_result.styled_dom.styled_nodes.as_container()[$node_id]
                                    .state;

                            let wh_config = WhConfig {
                                width: WidthConfig {
                                    exact: css_property_cache
                                        .get_width(node_data, &$node_id, styled_node_state)
                                        .and_then(|p| p.get_property().copied()),
                                    max: css_property_cache
                                        .get_max_width(node_data, &$node_id, styled_node_state)
                                        .and_then(|p| p.get_property().copied()),
                                    min: css_property_cache
                                        .get_min_width(node_data, &$node_id, styled_node_state)
                                        .and_then(|p| p.get_property().copied()),
                                    overflow: css_property_cache
                                        .get_overflow_x(node_data, &$node_id, styled_node_state)
                                        .and_then(|p| p.get_property().copied()),
                                },
                                height: HeightConfig::default(),
                            };
                            let parent_width = layout_result.preferred_widths.as_ref()[$parent_id]
                                .clone()
                                .unwrap_or(root_size.width as f32);
                            let parent_parent_overflow_x = css_property_cache
                                .get_overflow_x(
                                    &layout_result.styled_dom.node_data.as_container()
                                        [parent_parent],
                                    &parent_parent,
                                    &layout_result.styled_dom.styled_nodes.as_container()
                                        [parent_parent]
                                        .state,
                                )
                                .and_then(|p| p.get_property().copied())
                                .unwrap_or_default();

                            let new_preferred_width = determine_preferred_width(
                                &wh_config,
                                layout_result.preferred_widths.as_ref()[$node_id],
                                parent_width,
                                parent_parent_overflow_x,
                            );

                            if new_preferred_width != solved_width_layout.preferred_width {
                                preferred_width_changed = Some((
                                    solved_width_layout.preferred_width,
                                    new_preferred_width,
                                ));
                                solved_width_layout.preferred_width = new_preferred_width;
                            }
                        }

                        // recalculate min / max / preferred width constraint if needed
                        if changes_for_this_node.contains_key(&CssPropertyType::MinHeight)
                            || changes_for_this_node.contains_key(&CssPropertyType::MaxHeight)
                            || changes_for_this_node.contains_key(&CssPropertyType::Height)
                            || has_word_positions
                            || text_content_has_changed
                        {
                            let styled_node_state =
                                &layout_result.styled_dom.styled_nodes.as_container()[$node_id]
                                    .state;
                            let wh_config = WhConfig {
                                width: WidthConfig::default(),
                                height: HeightConfig {
                                    exact: css_property_cache
                                        .get_height(node_data, &$node_id, &styled_node_state)
                                        .and_then(|p| p.get_property().copied()),
                                    max: css_property_cache
                                        .get_max_height(node_data, &$node_id, &styled_node_state)
                                        .and_then(|p| p.get_property().copied()),
                                    min: css_property_cache
                                        .get_min_height(node_data, &$node_id, &styled_node_state)
                                        .and_then(|p| p.get_property().copied()),
                                    overflow: css_property_cache
                                        .get_overflow_y(node_data, &$node_id, &styled_node_state)
                                        .and_then(|p| p.get_property().copied()),
                                },
                            };
                            let parent_height = layout_result.preferred_heights.as_ref()
                                [$parent_id]
                                .clone()
                                .unwrap_or(root_size.height as f32);
                            let parent_parent_overflow_y = css_property_cache
                                .get_overflow_x(
                                    &layout_result.styled_dom.node_data.as_container()
                                        [parent_parent],
                                    &parent_parent,
                                    &layout_result.styled_dom.styled_nodes.as_container()
                                        [parent_parent]
                                        .state,
                                )
                                .and_then(|p| p.get_property().copied())
                                .unwrap_or_default();

                            let new_preferred_height = determine_preferred_height(
                                &wh_config,
                                layout_result.preferred_heights.as_ref()[$node_id],
                                parent_height,
                                parent_parent_overflow_y,
                            );

                            if new_preferred_height != solved_height_layout.preferred_height {
                                preferred_height_changed = Some((
                                    solved_height_layout.preferred_height,
                                    new_preferred_height,
                                ));
                                solved_height_layout.preferred_height = new_preferred_height;
                            }
                        }

                        // padding / margin horizontal change
                        if let Some(CssProperty::PaddingLeft(prop)) = changes_for_this_node
                            .get(&CssPropertyType::PaddingLeft)
                            .map(|p| &p.current_prop)
                        {
                            solved_width_layout.padding_left = Some(*prop);
                            padding_x_changed = true;
                        }

                        if let Some(CssProperty::PaddingRight(prop)) = changes_for_this_node
                            .get(&CssPropertyType::PaddingRight)
                            .map(|p| &p.current_prop)
                        {
                            solved_width_layout.padding_right = Some(*prop);
                            padding_x_changed = true;
                        }

                        if let Some(CssProperty::MarginLeft(prop)) = changes_for_this_node
                            .get(&CssPropertyType::MarginLeft)
                            .map(|p| &p.current_prop)
                        {
                            solved_width_layout.margin_left = Some(*prop);
                            margin_x_changed = true;
                        }

                        if let Some(CssProperty::MarginRight(prop)) = changes_for_this_node
                            .get(&CssPropertyType::MarginRight)
                            .map(|p| &p.current_prop)
                        {
                            solved_width_layout.margin_right = Some(*prop);
                            margin_x_changed = true;
                        }

                        // padding / margin vertical change
                        if let Some(CssProperty::PaddingTop(prop)) = changes_for_this_node
                            .get(&CssPropertyType::PaddingTop)
                            .map(|p| &p.current_prop)
                        {
                            solved_height_layout.padding_top = Some(*prop);
                            padding_y_changed = true;
                        }

                        if let Some(CssProperty::PaddingBottom(prop)) = changes_for_this_node
                            .get(&CssPropertyType::PaddingBottom)
                            .map(|p| &p.current_prop)
                        {
                            solved_height_layout.padding_bottom = Some(*prop);
                            padding_y_changed = true;
                        }

                        if let Some(CssProperty::MarginTop(prop)) = changes_for_this_node
                            .get(&CssPropertyType::MarginTop)
                            .map(|p| &p.current_prop)
                        {
                            solved_height_layout.margin_top = Some(*prop);
                            margin_y_changed = true;
                        }

                        if let Some(CssProperty::MarginBottom(prop)) = changes_for_this_node
                            .get(&CssPropertyType::MarginBottom)
                            .map(|p| &p.current_prop)
                        {
                            solved_height_layout.margin_bottom = Some(*prop);
                            margin_y_changed = true;
                        }

                        if let Some((previous_preferred_width, current_preferred_width)) =
                            preferred_width_changed
                        {
                            // need to recalc the width of the node
                            // need to bubble the width to the parent width
                            // need to recalc the width of all children
                            // need to recalc the x position of all siblings
                            parents_that_need_to_recalc_width_of_children.insert($parent_id);
                            nodes_that_need_to_bubble_width.insert(
                                $node_id,
                                (previous_preferred_width, current_preferred_width),
                            );
                            parents_that_need_to_recalc_width_of_children.insert($node_id);
                            parents_that_need_to_reposition_children_x.insert($parent_id);
                        }

                        if let Some((previous_preferred_height, current_preferred_height)) =
                            preferred_height_changed
                        {
                            // need to recalc the height of the node
                            // need to bubble the height of all current node siblings to the parent
                            // height need to recalc the height of all children
                            // need to recalc the y position of all siblings
                            parents_that_need_to_recalc_height_of_children.insert($parent_id);
                            nodes_that_need_to_bubble_height.insert(
                                $node_id,
                                (previous_preferred_height, current_preferred_height),
                            );
                            parents_that_need_to_recalc_height_of_children.insert($node_id);
                            parents_that_need_to_reposition_children_y.insert($parent_id);
                        }

                        if padding_x_changed {
                            // need to recalc the widths of all children
                            // need to recalc the x position of all children
                            parents_that_need_to_recalc_width_of_children.insert($node_id);
                            parents_that_need_to_reposition_children_x.insert($node_id);
                        }

                        if padding_y_changed {
                            // need to recalc the heights of all children
                            // need to bubble the height of all current node children to the
                            // current node min_inner_size_px
                            parents_that_need_to_recalc_height_of_children.insert($node_id);
                            parents_that_need_to_reposition_children_y.insert($node_id);
                        }

                        if margin_x_changed {
                            // need to recalc the widths of all siblings
                            // need to recalc the x positions of all siblings
                            parents_that_need_to_recalc_width_of_children.insert($parent_id);
                            parents_that_need_to_reposition_children_x.insert($parent_id);
                        }

                        if margin_y_changed {
                            // need to recalc the heights of all siblings
                            // need to recalc the y positions of all siblings
                            parents_that_need_to_recalc_height_of_children.insert($parent_id);
                            parents_that_need_to_reposition_children_y.insert($parent_id);
                        }

                        // TODO: absolute positions / top-left-right-bottom changes!
                    }
                }
            };
        }

        let node_id = match node_id.into_crate_internal() {
            Some(s) => s,
            None => continue,
        };

        let parent_id = layout_result.styled_dom.node_hierarchy.as_container()[node_id]
            .parent_id()
            .unwrap_or(layout_result.styled_dom.root.into_crate_internal().unwrap());

        detect_changes!(node_id, parent_id);

        for child_id in node_id.az_children(&layout_result.styled_dom.node_hierarchy.as_container())
        {
            detect_changes!(child_id, node_id);
        }
    }

    // for all nodes that changed, recalculate the min_inner_size_px of the parents
    // by re-bubbling the sizes to the parents (but only for the nodes that need it)
    let mut rebubble_parent_widths = BTreeMap::new();
    let mut rebubble_parent_heights = BTreeMap::new();

    for (node_id, (old_preferred_width, new_preferred_width)) in
        nodes_that_need_to_bubble_width.iter().rev()
    {
        if let Some(parent_id) =
            layout_result.styled_dom.node_hierarchy.as_container()[*node_id].parent_id()
        {
            let change = new_preferred_width.min_needed_space().unwrap_or(0.0)
                - old_preferred_width.min_needed_space().unwrap_or(0.0);
            layout_result.width_calculated_rects.as_ref_mut()[*node_id].min_inner_size_px =
                new_preferred_width.min_needed_space().unwrap_or(0.0);
            layout_result.width_calculated_rects.as_ref_mut()[parent_id].min_inner_size_px +=
                change;
            layout_result.width_calculated_rects.as_ref_mut()[parent_id].flex_grow_px = 0.0;
            if change != 0.0 {
                *rebubble_parent_widths
                    .entry(parent_id)
                    .or_insert_with(|| 0.0) += change;
                parents_that_need_to_recalc_width_of_children.insert(parent_id);
            }
        }
    }

    for (node_id, (old_preferred_height, new_preferred_height)) in
        nodes_that_need_to_bubble_height.iter().rev()
    {
        if let Some(parent_id) =
            layout_result.styled_dom.node_hierarchy.as_container()[*node_id].parent_id()
        {
            let change = new_preferred_height.min_needed_space().unwrap_or(0.0)
                - old_preferred_height.min_needed_space().unwrap_or(0.0);
            layout_result.height_calculated_rects.as_ref_mut()[*node_id].min_inner_size_px =
                new_preferred_height.min_needed_space().unwrap_or(0.0);
            layout_result.height_calculated_rects.as_ref_mut()[parent_id].min_inner_size_px +=
                change;
            layout_result.height_calculated_rects.as_ref_mut()[parent_id].flex_grow_px = 0.0;
            if change != 0.0 {
                *rebubble_parent_heights
                    .entry(parent_id)
                    .or_insert_with(|| 0.0) += change;
                parents_that_need_to_recalc_height_of_children.insert(parent_id);
            }
        }
    }

    let mut subtree_needs_relayout_width = BTreeSet::new();
    let mut subtree_needs_relayout_height = BTreeSet::new();

    // propagate width / height change from the inside out
    while !parents_that_need_to_recalc_width_of_children.is_empty() {
        // width_calculated_rect_arena_apply_flex_grow also
        // needs the parents parent to work correctly
        let parents_to_extend = parents_that_need_to_recalc_width_of_children
            .iter()
            .map(|p| {
                layout_result.styled_dom.node_hierarchy.as_container()[*p]
                    .parent_id()
                    .unwrap_or(NodeId::ZERO)
            })
            .collect::<BTreeSet<_>>();

        parents_that_need_to_recalc_width_of_children.extend(parents_to_extend.into_iter());

        let previous_widths = parents_that_need_to_recalc_width_of_children
            .iter()
            .filter_map(|node_id| {
                layout_result
                    .width_calculated_rects
                    .as_ref()
                    .get(*node_id)
                    .map(|s| (node_id, *s))
            })
            .collect::<BTreeMap<_, _>>();

        subtree_needs_relayout_width.extend(
            parents_that_need_to_recalc_width_of_children
                .iter()
                .cloned(),
        );

        width_calculated_rect_arena_apply_flex_grow(
            &mut layout_result.width_calculated_rects,
            &layout_result.styled_dom.node_hierarchy.as_container(),
            &layout_result.layout_displays.as_ref(),
            &layout_result.layout_flex_grows.as_ref(),
            &layout_result.layout_positions.as_ref(),
            &layout_result.layout_flex_directions.as_ref(),
            &layout_result.styled_dom.non_leaf_nodes.as_ref(),
            root_size.width as f32,
            // important - only recalc the widths necessary!
            &parents_that_need_to_recalc_width_of_children,
        );

        // if the parent width is not the same, bubble
        let parents_that_changed_width = parents_that_need_to_recalc_width_of_children
            .iter()
            .filter_map(|p| {
                // get the current width after relayout
                let current_width = layout_result
                    .width_calculated_rects
                    .as_ref()
                    .get(*p)
                    .copied()?;
                let previous_width = previous_widths.get(p).copied()?;
                if current_width == previous_width {
                    return None;
                }
                let parent_id =
                    layout_result.styled_dom.node_hierarchy.as_container()[*p].parent_id()?;
                Some(parent_id)
            })
            .collect();

        // loop while there are still widths that changed size
        parents_that_need_to_recalc_width_of_children = parents_that_changed_width;
    }

    parents_that_need_to_recalc_width_of_children = subtree_needs_relayout_width;

    while !parents_that_need_to_recalc_height_of_children.is_empty() {
        let parents_to_extend = parents_that_need_to_recalc_height_of_children
            .iter()
            .map(|p| {
                layout_result.styled_dom.node_hierarchy.as_container()[*p]
                    .parent_id()
                    .unwrap_or(NodeId::ZERO)
            })
            .collect::<BTreeSet<_>>();

        parents_that_need_to_recalc_height_of_children.extend(parents_to_extend.into_iter());

        let previous_heights = parents_that_need_to_recalc_height_of_children
            .iter()
            .filter_map(|node_id| {
                layout_result
                    .height_calculated_rects
                    .as_ref()
                    .get(*node_id)
                    .map(|s| (node_id, *s))
            })
            .collect::<BTreeMap<_, _>>();

        subtree_needs_relayout_height.extend(
            parents_that_need_to_recalc_height_of_children
                .iter()
                .cloned(),
        );

        height_calculated_rect_arena_apply_flex_grow(
            &mut layout_result.height_calculated_rects,
            &layout_result.styled_dom.node_hierarchy.as_container(),
            &layout_result.layout_displays.as_ref(),
            &layout_result.layout_flex_grows.as_ref(),
            &layout_result.layout_positions.as_ref(),
            &layout_result.layout_flex_directions.as_ref(),
            &layout_result.styled_dom.non_leaf_nodes.as_ref(),
            root_size.height as f32,
            // important - only recalc the heights necessary!
            &parents_that_need_to_recalc_height_of_children,
        );

        // if the parent height is not the same, bubble
        let mut parents_that_changed_height = parents_that_need_to_recalc_height_of_children
            .iter()
            .filter_map(|p| {
                // get the current height after relayout
                let current_height = layout_result
                    .height_calculated_rects
                    .as_ref()
                    .get(*p)
                    .copied()?;
                let previous_height = previous_heights.get(p).copied()?;
                if current_height == previous_height {
                    return None;
                }
                let parent_id =
                    layout_result.styled_dom.node_hierarchy.as_container()[*p].parent_id()?;
                Some(parent_id)
            })
            .collect();

        // loop while there are still heights that changed size
        parents_that_need_to_recalc_height_of_children = parents_that_changed_height;
    }

    parents_that_need_to_recalc_height_of_children = subtree_needs_relayout_height;

    // if a node has been modified then the entire subtree needs to be re-laid out
    for n in parents_that_need_to_recalc_width_of_children.clone() {
        let subtree_parents = layout_result.styled_dom.get_subtree_parents(n);
        for s in subtree_parents {
            parents_that_need_to_recalc_width_of_children.insert(s);
        }
        parents_that_need_to_reposition_children_x.insert(n);
    }

    for n in parents_that_need_to_recalc_height_of_children.clone() {
        let subtree_parents = layout_result.styled_dom.get_subtree_parents(n);
        for s in subtree_parents {
            parents_that_need_to_recalc_height_of_children.insert(s);
        }
        parents_that_need_to_reposition_children_y.insert(n);
    }

    for n in parents_that_need_to_reposition_children_x.clone() {
        let subtree_parents = layout_result.styled_dom.get_subtree_parents(n);
        for s in subtree_parents {
            parents_that_need_to_reposition_children_x.insert(s);
        }
    }

    for n in parents_that_need_to_reposition_children_y.clone() {
        let subtree_parents = layout_result.styled_dom.get_subtree_parents(n);
        for s in subtree_parents {
            parents_that_need_to_reposition_children_y.insert(s);
        }
    }

    // -- step 2: recalc position for those parents that need it

    width_calculated_rect_arena_apply_flex_grow(
        &mut layout_result.width_calculated_rects,
        &layout_result.styled_dom.node_hierarchy.as_container(),
        &layout_result.layout_displays.as_ref(),
        &layout_result.layout_flex_grows.as_ref(),
        &layout_result.layout_positions.as_ref(),
        &layout_result.layout_flex_directions.as_ref(),
        &layout_result.styled_dom.non_leaf_nodes.as_ref(),
        root_size.width as f32,
        // important - only recalc the widths necessary!
        &parents_that_need_to_recalc_width_of_children,
    );

    height_calculated_rect_arena_apply_flex_grow(
        &mut layout_result.height_calculated_rects,
        &layout_result.styled_dom.node_hierarchy.as_container(),
        &layout_result.layout_displays.as_ref(),
        &layout_result.layout_flex_grows.as_ref(),
        &layout_result.layout_positions.as_ref(),
        &layout_result.layout_flex_directions.as_ref(),
        &layout_result.styled_dom.non_leaf_nodes.as_ref(),
        root_size.height as f32,
        // important - only recalc the heights necessary!
        &parents_that_need_to_recalc_height_of_children,
    );

    // -- step 2: recalc position for those parents that need it

    get_x_positions(
        &mut layout_result.solved_pos_x,
        &layout_result.width_calculated_rects.as_ref(),
        &layout_result.styled_dom.node_hierarchy.as_container(),
        &layout_result.layout_positions.as_ref(),
        &layout_result.layout_flex_directions.as_ref(),
        &layout_result.layout_justify_contents.as_ref(),
        &layout_result.styled_dom.non_leaf_nodes.as_ref(),
        LogicalPosition::new(root_bounds.origin.x as f32, root_bounds.origin.y as f32),
        &parents_that_need_to_reposition_children_x, // <- important
    );

    get_y_positions(
        &mut layout_result.solved_pos_y,
        &layout_result.height_calculated_rects.as_ref(),
        &layout_result.styled_dom.node_hierarchy.as_container(),
        &layout_result.layout_positions.as_ref(),
        &layout_result.layout_flex_directions.as_ref(),
        &layout_result.layout_justify_contents.as_ref(),
        &layout_result.styled_dom.non_leaf_nodes.as_ref(),
        LogicalPosition::new(root_bounds.origin.x as f32, root_bounds.origin.y as f32),
        &parents_that_need_to_reposition_children_y, // <- important
    );

    // update positioned_word_cache
    let mut updated_word_caches = parents_that_need_to_recalc_width_of_children.clone();
    for parent_id in parents_that_need_to_recalc_width_of_children.iter() {
        for child_id in
            parent_id.az_children(&layout_result.styled_dom.node_hierarchy.as_container())
        {
            // if max_width_changed { } // - optimization?
            updated_word_caches.insert(child_id);
        }
    }

    updated_word_caches.extend(node_ids_that_changed_text_content.clone().into_iter());

    #[cfg(feature = "text_layout")]
    let text_layout_options = extract_text_layout_options(&layout_result.styled_dom);
    #[cfg(feature = "text_layout")]
    create_word_positions(
        &mut layout_result.positioned_words_cache,
        &updated_word_caches,
        renderer_resources,
        &layout_result.words_cache,
        &layout_result.shaped_words_cache,
        &layout_result.styled_dom,
        Some(&layout_result.width_calculated_rects.as_ref()),
        &text_layout_options,
        debug_messages,
    );

    // determine which nodes changed their size and return
    let mut nodes_that_changed_size = BTreeSet::new();
    for parent_id in parents_that_need_to_recalc_width_of_children {
        nodes_that_changed_size.insert(parent_id);
        for child_id in
            parent_id.az_children(&layout_result.styled_dom.node_hierarchy.as_container())
        {
            nodes_that_changed_size.insert(child_id);
        }
    }
    for parent_id in parents_that_need_to_recalc_height_of_children {
        nodes_that_changed_size.insert(parent_id);
        for child_id in
            parent_id.az_children(&layout_result.styled_dom.node_hierarchy.as_container())
        {
            nodes_that_changed_size.insert(child_id);
        }
    }
    for node_text_content_changed in &node_ids_that_changed_text_content {
        let parent = layout_result.styled_dom.node_hierarchy.as_container()
            [*node_text_content_changed]
            .parent_id()
            .unwrap_or(NodeId::ZERO);
        nodes_that_changed_size.insert(parent);
    }
    nodes_that_changed_size.extend(node_ids_that_changed_text_content.into_iter());

    let css_property_cache = layout_result.styled_dom.get_css_property_cache();
    let node_data_container = layout_result.styled_dom.node_data.as_container();

    let mut all_offsets_to_recalc = BTreeMap::new();
    for node_id in nodes_that_changed_size.iter() {
        all_offsets_to_recalc.entry(*node_id).or_insert_with(|| {
            let styled_node_state =
                &layout_result.styled_dom.styled_nodes.as_container()[*node_id].state;
            precalculate_offset(
                &node_data_container[*node_id],
                &css_property_cache,
                node_id,
                styled_node_state,
            )
        });

        for child_id in node_id.az_children(&layout_result.styled_dom.node_hierarchy.as_container())
        {
            all_offsets_to_recalc.entry(child_id).or_insert_with(|| {
                let styled_node_state =
                    &layout_result.styled_dom.styled_nodes.as_container()[child_id].state;
                precalculate_offset(
                    &node_data_container[*node_id],
                    &css_property_cache,
                    &child_id,
                    styled_node_state,
                )
            });
        }
    }

    // update layout_result.rects and layout_result.glyph_cache
    // if positioned_word_cache changed, regenerate layouted_glyph_cache
    position_nodes(
        &mut layout_result.rects.as_ref_mut(),
        &layout_result.styled_dom,
        AllOffsetsProvider::OnlyRecalculatedNodes(&all_offsets_to_recalc),
        &layout_result.width_calculated_rects.as_ref(),
        &layout_result.height_calculated_rects.as_ref(),
        &layout_result.solved_pos_x.as_ref(),
        &layout_result.solved_pos_y.as_ref(),
        &nodes_that_changed_size,
        &nodes_that_changed_size,
        &layout_result.layout_positions.as_ref(),
        &layout_result.words_cache,
        &layout_result.shaped_words_cache,
        &layout_result.positioned_words_cache,
        document_id,
        debug_messages,
    );

    layout_result.root_size = root_bounds.size;
    layout_result.root_position = root_bounds.origin;

    if !nodes_that_changed_size.is_empty() {
        // TODO: optimize?
        get_nodes_that_need_scroll_clip(
            &mut layout_result.scrollable_nodes,
            &layout_result.styled_dom.styled_nodes.as_container(),
            &layout_result.styled_dom.node_data.as_container(),
            &layout_result.styled_dom.node_hierarchy.as_container(),
            &layout_result.rects.as_ref(),
            &layout_result.styled_dom.non_leaf_nodes.as_ref(),
            dom_id,
            document_id,
        );
    }

    let gpu_key_changes = layout_result
        .gpu_value_cache
        .synchronize(&layout_result.rects.as_ref(), &layout_result.styled_dom);

    let resized_nodes = nodes_that_changed_size.into_iter().collect();

    RelayoutChanges {
        resized_nodes,
        gpu_key_changes,
    }
}

/// Extract text layout options from the styled DOM based on NodeId
fn extract_text_layout_options(
    styled_dom: &StyledDom,
) -> BTreeMap<NodeId, ResolvedTextLayoutOptions> {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();

    node_data_container
        .internal
        .iter()
        .enumerate()
        .filter_map(|(node_id, node_data)| {
            let node_id = NodeId::new(node_id);
            let styled_node_state = &styled_nodes[node_id].state;

            // Only extract options for text nodes
            if let NodeType::Text(_) = node_data.get_node_type() {
                // Extract standard text layout options
                let font_size = css_property_cache.get_font_size_or_default(
                    node_data,
                    &node_id,
                    styled_node_state,
                );
                let font_size_px = font_size.inner.to_pixels(DEFAULT_FONT_SIZE_PX as f32);

                let letter_spacing = css_property_cache
                    .get_letter_spacing(node_data, &node_id, styled_node_state)
                    .and_then(|ls| {
                        Some(
                            ls.get_property()?
                                .inner
                                .to_pixels(azul_core::ui_solver::DEFAULT_LETTER_SPACING),
                        )
                    });

                let word_spacing = css_property_cache
                    .get_word_spacing(node_data, &node_id, styled_node_state)
                    .and_then(|ws| Some(ws.get_property()?.inner.to_pixels(DEFAULT_WORD_SPACING)));

                let line_height = css_property_cache
                    .get_line_height(node_data, &node_id, styled_node_state)
                    .and_then(|lh| Some(lh.get_property()?.inner.get()));

                let tab_width = css_property_cache
                    .get_tab_width(node_data, &node_id, styled_node_state)
                    .and_then(|tw| Some(tw.get_property()?.inner.get()));

                // Extract width constraint
                let mut max_width = None;
                let mut cur_node = node_id;
                while let Some(parent) =
                    styled_dom.node_hierarchy.as_container()[node_id].parent_id()
                {
                    let overflow_x = css_property_cache
                        .get_overflow_x(
                            &node_data_container[parent],
                            &parent,
                            &styled_nodes[parent].state,
                        )
                        .cloned();

                    match overflow_x {
                        Some(CssPropertyValue::Exact(LayoutOverflow::Hidden))
                        | Some(CssPropertyValue::Exact(LayoutOverflow::Visible)) => {
                            max_width = None;
                            break;
                        }
                        None
                        | Some(CssPropertyValue::None)
                        | Some(CssPropertyValue::Auto)
                        | Some(CssPropertyValue::Initial)
                        | Some(CssPropertyValue::Inherit)
                        | Some(CssPropertyValue::Exact(LayoutOverflow::Auto))
                        | Some(CssPropertyValue::Exact(LayoutOverflow::Scroll)) => {
                            // We'll set this later when solved widths are available
                            max_width = Some(1000.0); // Placeholder
                            break;
                        }
                    }

                    cur_node = parent;
                }

                // NEW: Extract text direction (RTL/LTR)
                let direction = css_property_cache
                    .get_direction(node_data, &node_id, styled_node_state)
                    .and_then(|d| d.get_property().copied())
                    .unwrap_or(TextDirection::LTR);

                let is_rtl = match direction {
                    Some(TextDirection::RTL) => ScriptType::RTL,
                    Some(TextDirection::LTR) => ScriptType::LTR,
                    _ => ScriptType::Mixed, // Auto-detect
                };

                let text_justify = css_property_cache
                    .get_text_align(node_data, &node_id, styled_node_state)
                    .and_then(|ta| ta.get_property().copied());

                let white_space = css_property_cache
                    .get_white_space(node_data, &node_id, styled_node_state)
                    .and_then(|ws| ws.get_property().copied());

                let can_break = white_space == Some(LayoutWhiteSpace::Nowrap);

                let hyphens = css_property_cache
                    .get_hyphens(node_data, &node_id, styled_node_state)
                    .and_then(|h| h.get_property().copied());

                let can_hyphenate = hyphens != Some(Hyphens::None);

                let options = ResolvedTextLayoutOptions {
                    font_size_px,
                    line_height: line_height.into(),
                    letter_spacing: letter_spacing.into(),
                    word_spacing: word_spacing.into(),
                    tab_width: tab_width.into(),
                    max_horizontal_width: max_width.into(),
                    leading: None.into(),
                    holes: Vec::new().into(),
                    max_vertical_height: None.into(),
                    can_break,
                    can_hyphenate,
                    hyphenation_character: Some('-' as u32).into(),
                    is_rtl,
                    text_justify,
                };

                Some((node_id, options))
            } else {
                None
            }
        })
        .collect()
}

/// Data structure to track inline elements
#[derive(Debug, Clone)]
pub struct InlineElement {
    pub node_id: NodeId,
    pub position: LogicalPosition,
    pub size: LogicalSize,
    pub is_text: bool,
}

/// Process and layout inline elements within a container
pub fn layout_inline_elements(
    container_node_id: NodeId,
    styled_dom: &StyledDom,
    width_calculated_rects: &NodeDataContainerRef<WidthCalculatedRect>,
    height_calculated_rects: &NodeDataContainerRef<HeightCalculatedRect>,
    x_positions: &NodeDataContainerRef<HorizontalSolvedPosition>,
    y_positions: &NodeDataContainerRef<VerticalSolvedPosition>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Vec<InlineElement> {
    // Get container dimensions
    let container_rect = PositionedRectangle {
        size: LogicalSize::new(
            width_calculated_rects[container_node_id].total(),
            height_calculated_rects[container_node_id].total(),
        ),
        position: PositionInfo::Static(PositionInfoInner {
            x_offset: x_positions[container_node_id].0,
            y_offset: y_positions[container_node_id].0,
            static_x_offset: x_positions[container_node_id].0,
            static_y_offset: y_positions[container_node_id].0,
        }),
        padding: Default::default(),
        margin: Default::default(),
        border_widths: Default::default(),
        box_shadow: Default::default(),
        box_sizing: Default::default(),
        resolved_text_layout_options: None,
        overflow_x: Default::default(),
        overflow_y: Default::default(),
    };

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Laying out inline elements for container node {}",
                container_node_id.index()
            )
            .into(),
            location: "layout_inline_elements".to_string().into(),
        });
    }

    // Get relevant display info for all nodes
    let display_values = get_display_values(styled_dom);

    // Find all direct children that are inline or inline-block
    let mut inline_elements = Vec::new();
    let children = container_node_id
        .az_children(&styled_dom.node_hierarchy.as_container())
        .collect::<Vec<_>>();

    // Handle RTL direction for the container
    let direction = get_container_direction(styled_dom, container_node_id);
    let is_rtl = direction == TextDirection::RTL;

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Container direction: {:?}, child count: {}",
                direction,
                children.len()
            )
            .into(),
            location: "layout_inline_elements".to_string().into(),
        });
    }

    // First pass: collect all inline elements
    for child_id in &children {
        let display = display_values
            .get(child_id)
            .cloned()
            .unwrap_or(LayoutDisplay::Block);

        if display == LayoutDisplay::Inline || display == LayoutDisplay::InlineBlock {
            let child_width = width_calculated_rects[*child_id].total();
            let child_height = height_calculated_rects[*child_id].total();

            // Determine if this is a text node
            let is_text = match &styled_dom.node_data.as_container()[*child_id].get_node_type() {
                NodeType::Text(_) => true,
                _ => false,
            };

            inline_elements.push(InlineElement {
                node_id: *child_id,
                position: LogicalPosition::zero(), // Will be calculated in second pass
                size: LogicalSize::new(child_width, child_height),
                is_text,
            });

            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: format!(
                        "Found inline element: node {}, size: {}x{}, is_text: {}",
                        child_id.index(),
                        child_width,
                        child_height,
                        is_text
                    )
                    .into(),
                    location: "layout_inline_elements".to_string().into(),
                });
            }
        }
    }

    // Second pass: layout the inline elements
    let container_padding_left = get_container_padding_left(
        container_node_id,
        width_calculated_rects,
        container_rect.size.width,
    );
    let container_padding_top = get_container_padding_top(
        container_node_id,
        height_calculated_rects,
        container_rect.size.height,
    );

    let mut current_x = container_padding_left;
    let mut current_y = container_padding_top;
    let mut line_height = 0.0;
    let max_width = container_rect.size.width
        - container_padding_left
        - get_container_padding_right(
            container_node_id,
            width_calculated_rects,
            container_rect.size.width,
        );

    // Store elements for each line to handle RTL layout
    let mut current_line_elements = Vec::new();

    for inline_element in &inline_elements {
        // Check if element fits on the current line
        if current_x + inline_element.size.width > max_width && current_x > container_padding_left {
            // Doesn't fit, move to next line
            // First, lay out the current line (for RTL support)
            layout_current_line(&mut current_line_elements, is_rtl, current_x, max_width);
            current_line_elements.clear();

            current_y += line_height;
            current_x = container_padding_left;
            line_height = 0.0;
        }

        // Add element to current position
        let element_position = LogicalPosition::new(current_x, current_y);
        current_line_elements.push((inline_element.node_id, element_position));

        // Update position and track maximum line height
        current_x += inline_element.size.width;
        line_height = line_height.max(inline_element.size.height);
    }

    // Layout the last line if needed
    if !current_line_elements.is_empty() {
        layout_current_line(&mut current_line_elements, is_rtl, current_x, max_width);
    }

    // Update positions in the inline_elements vector
    for (node_id, position) in current_line_elements {
        for element in &mut inline_elements {
            if element.node_id == node_id {
                element.position = position;
                break;
            }
        }
    }

    inline_elements
}

// Helper function to layout a single line of inline elements
fn layout_current_line(
    line_elements: &mut Vec<(NodeId, LogicalPosition)>,
    is_rtl: bool,
    line_width: f32,
    max_width: f32,
) {
    if !is_rtl {
        return; // No adjustment needed for LTR
    }

    // For RTL, we need to reposition elements from right to left
    // Calculate the space to leave on the left side
    let space_on_left = max_width - line_width;

    // Reverse positions
    let elements_count = line_elements.len();
    for i in 0..elements_count {
        let (node_id, mut position) = line_elements[i];
        position.x = max_width - position.x - space_on_left;
        line_elements[i] = (node_id, position);
    }
}

// Helper function to get text direction for a container
fn get_container_direction(styled_dom: &StyledDom, node_id: NodeId) -> TextDirection {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    // Check for direction property
    let direction = css_property_cache
        .get_direction(node_data, &node_id, styled_node_state)
        .and_then(|d| d.get_property().copied());

    match direction {
        Some(TextDirection::RTL) => TextDirection::RTL,
        _ => TextDirection::LTR, // Default to LTR
    }
}

// Helper to get display values from the DOM
fn get_display_values(styled_dom: &StyledDom) -> BTreeMap<NodeId, LayoutDisplay> {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();

    let mut display_values = BTreeMap::new();

    for (node_id, styled_node) in styled_nodes.internal.iter().enumerate() {
        let node_id = NodeId::new(node_id);

        // Convert from LayoutDisplay to our Display enum
        let layout_display = css_property_cache
            .get_display(&node_data_container[node_id], &node_id, &styled_node.state)
            .and_then(|d| d.get_property().cloned())
            .unwrap_or(LayoutDisplay::Block);

        display_values.insert(node_id, display);
    }

    display_values
}

// Helper functions to get container padding
fn get_container_padding_left(
    node_id: NodeId,
    width_rects: &NodeDataContainerRef<WidthCalculatedRect>,
    container_width: f32,
) -> f32 {
    width_rects[node_id]
        .padding_left
        .and_then(|p| p.get_property().map(|v| v.inner.to_pixels(container_width)))
        .unwrap_or(0.0)
}

fn get_container_padding_right(
    node_id: NodeId,
    width_rects: &NodeDataContainerRef<WidthCalculatedRect>,
    container_width: f32,
) -> f32 {
    width_rects[node_id]
        .padding_right
        .and_then(|p| p.get_property().map(|v| v.inner.to_pixels(container_width)))
        .unwrap_or(0.0)
}

fn get_container_padding_top(
    node_id: NodeId,
    height_rects: &NodeDataContainerRef<HeightCalculatedRect>,
    container_height: f32,
) -> f32 {
    height_rects[node_id]
        .padding_top
        .and_then(|p| {
            p.get_property()
                .map(|v| v.inner.to_pixels(container_height))
        })
        .unwrap_or(0.0)
}

fn get_container_padding_bottom(
    node_id: NodeId,
    height_rects: &NodeDataContainerRef<HeightCalculatedRect>,
    container_height: f32,
) -> f32 {
    height_rects[node_id]
        .padding_bottom
        .and_then(|p| {
            p.get_property()
                .map(|v| v.inner.to_pixels(container_height))
        })
        .unwrap_or(0.0)
}

// NEW: Process inline elements after basic layout
fn process_inline_elements(
    styled_dom: &StyledDom,
    width_calculated_rects: &mut NodeDataContainer<WidthCalculatedRect>,
    height_calculated_rects: &mut NodeDataContainer<HeightCalculatedRect>,
    x_positions: &NodeDataContainerRef<HorizontalSolvedPosition>,
    y_positions: &NodeDataContainerRef<VerticalSolvedPosition>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) {
    use crate::text::layout::{adjust_sizes_after_inline_layout, layout_inline_elements};

    // Get display info for all nodes
    let display_values = get_display_values(styled_dom);

    // Find container nodes that may have inline children
    let potential_containers = styled_dom
        .non_leaf_nodes
        .iter()
        .filter_map(|node| node.node_id.into_crate_internal())
        .filter(|node_id| {
            // Check if node has any inline children
            node_id
                .az_children(&styled_dom.node_hierarchy.as_container())
                .any(|child_id| {
                    let display = display_values.get(&child_id).unwrap_or(&Display::Block);
                    *display == Display::Inline || *display == Display::InlineBlock
                })
        })
        .collect::<Vec<_>>();

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Processing {} containers with inline elements",
                potential_containers.len()
            )
            .into(),
            location: "process_inline_elements".to_string().into(),
        });
    }

    // Process each container with inline elements
    for container_id in potential_containers {
        // Layout the inline elements within this container
        let inline_elements = layout_inline_elements(
            container_id,
            styled_dom,
            &width_calculated_rects.as_ref(),
            &height_calculated_rects.as_ref(),
            x_positions,
            y_positions,
            debug_messages,
        );

        if !inline_elements.is_empty() {
            // Adjust container and parent sizes based on inline layout
            adjust_sizes_after_inline_layout(
                container_id,
                styled_dom,
                &mut width_calculated_rects.as_ref_mut(),
                &mut height_calculated_rects.as_ref_mut(),
                &inline_elements,
                debug_messages,
            );
        }
    }
}

// Adjust parent and sibling sizes after inline layout
pub fn adjust_sizes_after_inline_layout(
    container_node_id: NodeId,
    styled_dom: &StyledDom,
    width_calculated_rects: &mut NodeDataContainerRefMut<WidthCalculatedRect>,
    height_calculated_rects: &mut NodeDataContainerRefMut<HeightCalculatedRect>,
    inline_elements: &[InlineElement],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) {
    if inline_elements.is_empty() {
        return;
    }

    // Calculate the total bounds of all inline elements
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for element in inline_elements {
        min_x = min_x.min(element.position.x);
        min_y = min_y.min(element.position.y);
        max_x = max_x.max(element.position.x + element.size.width);
        max_y = max_y.max(element.position.y + element.size.height);
    }

    // Calculate container padding
    let container_width = width_calculated_rects[container_node_id].total();
    let container_height = height_calculated_rects[container_node_id].total();

    let padding_left = get_container_padding_left(
        container_node_id,
        width_calculated_rects.as_ref(),
        container_width,
    );
    let padding_right = get_container_padding_right(
        container_node_id,
        width_calculated_rects.as_ref(),
        container_width,
    );
    let padding_top = get_container_padding_top(
        container_node_id,
        &height_calculated_rects.as_borrowing_ref(),
        container_height,
    );
    let padding_bottom = get_container_padding_bottom(
        container_node_id,
        &height_calculated_rects.as_borrowing_ref(),
        container_height,
    );

    // Calculate the content width and height needed
    let content_width = max_x - min_x;
    let content_height = max_y - min_y;

    // Calculate the total width and height needed (including padding)
    let total_width_needed = content_width + padding_left + padding_right;
    let total_height_needed = content_height + padding_top + padding_bottom;

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Adjusting container size after inline layout. Original: {}x{}, New: {}x{}",
                container_width, container_height, total_width_needed, total_height_needed
            )
            .into(),
            location: "adjust_sizes_after_inline_layout".to_string().into(),
        });
    }

    // Expand the container if needed to fit inline content
    if total_width_needed > container_width {
        width_calculated_rects[container_node_id].min_inner_size_px =
            content_width.max(width_calculated_rects[container_node_id].min_inner_size_px);

        width_calculated_rects[container_node_id].flex_grow_px = (total_width_needed
            - width_calculated_rects[container_node_id].min_inner_size_px)
            .max(0.0);
    }

    if total_height_needed > container_height {
        height_calculated_rects[container_node_id].min_inner_size_px =
            content_height.max(height_calculated_rects[container_node_id].min_inner_size_px);

        height_calculated_rects[container_node_id].flex_grow_px = (total_height_needed
            - height_calculated_rects[container_node_id].min_inner_size_px)
            .max(0.0);
    }
}

// Function to update node positions after inline layout
pub fn update_inline_element_positions(
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    inline_elements: &[InlineElement],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) {
    for element in inline_elements {
        if let Some(rect) = positioned_rects.as_ref_mut().get_mut(element.node_id) {
            // Update the position based on inline layout calculation
            match &mut rect.position {
                PositionInfo::Static(info) | PositionInfo::Relative(info) => {
                    info.x_offset = element.position.x;
                    info.y_offset = element.position.y;
                    info.static_x_offset = element.position.x;
                    info.static_y_offset = element.position.y;

                    if let Some(messages) = debug_messages {
                        messages.push(LayoutDebugMessage {
                            message: format!(
                                "Updated inline element position: node {}, position: {:?}",
                                element.node_id.index(),
                                element.position
                            ),
                            location: "update_inline_element_positions".to_string(),
                        });
                    }
                }
                // Don't adjust absolute or fixed positioned elements
                _ => {}
            }
        }
    }
}
