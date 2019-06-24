use std::{
    collections::BTreeMap,
};
use webrender::api::{
    Epoch, ImageData, AddImage, ExternalImageId, ExternalImageData,
    ExternalImageType, TextureTarget,
};
use azul_css::{
    Css, LayoutPosition, CssProperty, ColorU, BoxShadowClipMode,
    RectStyle, RectLayout, CssPropertyValue, LayoutPoint, LayoutSize, LayoutRect,
};
use {
    FastHashMap,
    app_resources::{AppResources, AddImageMsg, FontImageApi},
    callbacks::{IFrameCallback, GlCallback, StackCheckedPointer},
    ui_state::UiState,
    ui_description::{UiDescription, StyledNode},
    id_tree::{NodeDataContainer, NodeId, NodeHierarchy},
    dom::{
        DomId, NodeData, ScrollTagId, DomString,
        NodeType::{self, Div, Text, Image, GlTexture, IFrame, Label},
    },
    ui_solver::do_the_layout,
    compositor::new_opengl_texture_id,
    window::{Window, WindowSize, FakeWindow},
    callbacks::LayoutInfo,
    text_layout::LayoutedGlyphs,
};
use azul_core::{
    callbacks::PipelineId,
    app_resources::{ImageId, FontInstanceKey},
    ui_solver::{
        PositionedRectangle, ResolvedOffsets, ExternalScrollId,
        LayoutResult, ScrolledNodes, OverflowingScrollNode
    },
    display_list::{
        CachedDisplayList, DisplayListMsg, LayoutRectContent,
        ImageRendering, AlphaType, DisplayListFrame, StyleBoxShadow, DisplayListScrollFrame,
        StyleBorderStyles, StyleBorderColors, StyleBorderRadius, StyleBorderWidths,
    },
};
use azul_layout::{GetStyle, style::Style};

pub(crate) struct DisplayList<'a, T: 'a> {
    pub(crate) ui_descr: &'a UiDescription<T>,
    pub(crate) rectangles: NodeDataContainer<DisplayRectangle<'a>>
}

/// Since the display list can take a lot of parameters, we don't want to
/// continually pass them as parameters of the function and rather use a
/// struct to pass them around. This is purely for ergonomic reasons.
///
/// `DisplayListParametersRef` has only members that are
///  **immutable references** to other things that need to be passed down the display list
#[derive(Clone)]
struct DisplayListParametersRef<'a, 'b, 'c, 'd, T: 'a> {
    /// ID of this Dom
    pub dom_id: DomId,
    /// Reference to original DOM data
    pub node_data: &'a NodeDataContainer<NodeData<T>>,
    /// The CSS that should be applied to the DOM
    pub css: &'b Css,
    /// Reference to the arena that contains all the styled rectangles
    pub display_rectangle_arena: &'c NodeDataContainer<DisplayRectangle<'d>>,
    /// Reference to the arena that contains the node hierarchy data, so
    /// that the node hierarchy can be re-used
    pub node_hierarchy: &'d NodeHierarchy,
    /// The current pipeline of the display list
    pub pipeline_id: PipelineId,
}

/// Same as `DisplayListParametersRef`, but for `&mut Something`
///
/// Note: The `'a` in the `'a + Layout` is technically not required.
/// Only rustc 1.28 requires this, more modern compiler versions insert it automatically.
struct DisplayListParametersMut<'a, T: 'a, U: FontImageApi> {
    /// Needs to be present, because the dom_to_displaylist_builder
    /// could call (recursively) a sub-DOM function again, for example an OpenGL callback
    pub app_data: &'a mut T,
    /// The app resources, so that a sub-DOM / iframe can register fonts and images
    /// TODO: How to handle cleanup ???
    pub app_resources: &'a mut AppResources,
    /// The OpenGL callback can push textures / images into the display list, however,
    /// those texture IDs have to be submitted to the actual Render API before drawing
    pub image_resource_updates: &'a mut BTreeMap<DomId, Vec<(ImageId, AddImageMsg)>>,
    /// Window access, so that sub-items can register OpenGL textures
    pub fake_window: &'a mut FakeWindow<T>,
    /// The render API that fonts and images should be added onto.
    pub render_api: &'a mut U,
    /// Laid out words and rectangles (contains info about content bounds and text layout)
    pub layout_result: &'a mut BTreeMap<DomId, LayoutResult>,
    /// Stores all scrollable nodes / hit-testable nodes on a per-DOM basis
    pub scrollable_nodes: &'a mut BTreeMap<DomId, ScrolledNodes>,
}

/// DisplayRectangle is the main type which the layout parsing step gets operated on.
#[derive(Debug)]
pub(crate) struct DisplayRectangle<'a> {
    /// `Some(id)` if this rectangle has a callback attached to it
    /// Note: this is not the same as the `NodeId`!
    /// These two are completely separate numbers!
    pub tag: Option<u64>,
    /// The original styled node
    pub(crate) styled_node: &'a StyledNode,
    /// The style properties of the node, parsed
    pub(crate) style: RectStyle,
    /// The layout properties of the node, parsed
    pub(crate) layout: RectLayout,
}

impl<'a> DisplayRectangle<'a> {
    #[inline]
    pub fn new(tag: Option<u64>, styled_node: &'a StyledNode) -> Self {
        Self { tag, styled_node, style: RectStyle::default(), layout: RectLayout::default() }
    }
}

impl<'a> GetStyle for DisplayRectangle<'a> {

    fn get_style(&self) -> Style {

        use azul_layout::{style::*, Size, Offsets, Number};
        use azul_css::{
            PixelValue, LayoutDisplay, LayoutDirection, LayoutWrap,
            LayoutAlignItems, LayoutAlignContent, LayoutJustifyContent,
            LayoutBoxSizing, Overflow as LayoutOverflow,
        };
        use azul_core::ui_solver::DEFAULT_FONT_SIZE;

        let rect_layout = &self.layout;
        let rect_style = &self.style;

        #[inline]
        fn translate_dimension(input: Option<CssPropertyValue<PixelValue>>) -> Dimension {
            use azul_css::{SizeMetric, EM_HEIGHT, PT_TO_PX};
            match input {
                None => Dimension::Undefined,
                Some(CssPropertyValue::Auto) => Dimension::Auto,
                Some(CssPropertyValue::None) => Dimension::Pixels(0.0),
                Some(CssPropertyValue::Initial) => Dimension::Undefined,
                Some(CssPropertyValue::Inherit) => Dimension::Undefined,
                Some(CssPropertyValue::Exact(pixel_value)) => match pixel_value.metric {
                    SizeMetric::Px => Dimension::Pixels(pixel_value.number.get()),
                    SizeMetric::Percent => Dimension::Percent(pixel_value.number.get()),
                    SizeMetric::Pt => Dimension::Pixels(pixel_value.number.get() * PT_TO_PX),
                    SizeMetric::Em => Dimension::Pixels(pixel_value.number.get() * EM_HEIGHT),
                }
            }
        }

        Style {
            display: match rect_layout.display {
                None => Display::Flex,
                Some(CssPropertyValue::Auto) => Display::Flex,
                Some(CssPropertyValue::None) => Display::None,
                Some(CssPropertyValue::Initial) => Display::Flex,
                Some(CssPropertyValue::Inherit) => Display::Flex,
                Some(CssPropertyValue::Exact(LayoutDisplay::Flex)) => Display::Flex,
                Some(CssPropertyValue::Exact(LayoutDisplay::Inline)) => Display::Inline,
            },
            box_sizing: match rect_layout.box_sizing.unwrap_or_default().get_property_or_default() {
                None => BoxSizing::ContentBox,
                Some(LayoutBoxSizing::ContentBox) => BoxSizing::ContentBox,
                Some(LayoutBoxSizing::BorderBox) => BoxSizing::BorderBox,
            },
            position_type: match rect_layout.position.unwrap_or_default().get_property_or_default() {
                Some(LayoutPosition::Static) => PositionType::Relative, // todo - static?
                Some(LayoutPosition::Relative) => PositionType::Relative,
                Some(LayoutPosition::Absolute) => PositionType::Absolute,
                None => PositionType::Relative,
            },
            direction: Direction::LTR,
            flex_direction: match rect_layout.direction.unwrap_or_default().get_property_or_default() {
                Some(LayoutDirection::Row) => FlexDirection::Row,
                Some(LayoutDirection::RowReverse) => FlexDirection::RowReverse,
                Some(LayoutDirection::Column) => FlexDirection::Column,
                Some(LayoutDirection::ColumnReverse) => FlexDirection::ColumnReverse,
                None => FlexDirection::Row,
            },
            flex_wrap: match rect_layout.wrap.unwrap_or_default().get_property_or_default() {
                Some(LayoutWrap::Wrap) => FlexWrap::Wrap,
                Some(LayoutWrap::NoWrap) => FlexWrap::NoWrap,
                None => FlexWrap::Wrap,
            },
            overflow: match rect_layout.overflow_x.unwrap_or_default().get_property_or_default() {
                Some(LayoutOverflow::Scroll) => Overflow::Scroll,
                Some(LayoutOverflow::Auto) => Overflow::Scroll,
                Some(LayoutOverflow::Hidden) => Overflow::Hidden,
                Some(LayoutOverflow::Visible) => Overflow::Visible,
                None => Overflow::Scroll,
            },
            align_items: match rect_layout.align_items.unwrap_or_default().get_property_or_default() {
                Some(LayoutAlignItems::Stretch) => AlignItems::Stretch,
                Some(LayoutAlignItems::Center) => AlignItems::Center,
                Some(LayoutAlignItems::Start) => AlignItems::FlexStart,
                Some(LayoutAlignItems::End) => AlignItems::FlexEnd,
                None => AlignItems::FlexStart,
            },
            align_content: match rect_layout.align_content.unwrap_or_default().get_property_or_default() {
                Some(LayoutAlignContent::Stretch) => AlignContent::Stretch,
                Some(LayoutAlignContent::Center) => AlignContent::Center,
                Some(LayoutAlignContent::Start) => AlignContent::FlexStart,
                Some(LayoutAlignContent::End) => AlignContent::FlexEnd,
                Some(LayoutAlignContent::SpaceBetween) => AlignContent::SpaceBetween,
                Some(LayoutAlignContent::SpaceAround) => AlignContent::SpaceAround,
                None => AlignContent::Stretch,
            },
            justify_content: match rect_layout.justify_content.unwrap_or_default().get_property_or_default() {
                Some(LayoutJustifyContent::Center) => JustifyContent::Center,
                Some(LayoutJustifyContent::Start) => JustifyContent::FlexStart,
                Some(LayoutJustifyContent::End) => JustifyContent::FlexEnd,
                Some(LayoutJustifyContent::SpaceBetween) => JustifyContent::SpaceBetween,
                Some(LayoutJustifyContent::SpaceAround) => JustifyContent::SpaceAround,
                Some(LayoutJustifyContent::SpaceEvenly) => JustifyContent::SpaceEvenly,
                None => JustifyContent::FlexStart,
            },
            position: Offsets {
                left: translate_dimension(rect_layout.left.map(|prop| prop.map_property(|l| l.0))),
                right: translate_dimension(rect_layout.right.map(|prop| prop.map_property(|r| r.0))),
                top: translate_dimension(rect_layout.top.map(|prop| prop.map_property(|t| t.0))),
                bottom: translate_dimension(rect_layout.bottom.map(|prop| prop.map_property(|b| b.0))),
            },
            margin: Offsets {
                left: translate_dimension(rect_layout.margin_left.map(|prop| prop.map_property(|l| l.0))),
                right: translate_dimension(rect_layout.margin_right.map(|prop| prop.map_property(|r| r.0))),
                top: translate_dimension(rect_layout.margin_top.map(|prop| prop.map_property(|t| t.0))),
                bottom: translate_dimension(rect_layout.margin_bottom.map(|prop| prop.map_property(|b| b.0))),
            },
            padding: Offsets {
                left: translate_dimension(rect_layout.padding_left.map(|prop| prop.map_property(|l| l.0))),
                right: translate_dimension(rect_layout.padding_right.map(|prop| prop.map_property(|r| r.0))),
                top: translate_dimension(rect_layout.padding_top.map(|prop| prop.map_property(|t| t.0))),
                bottom: translate_dimension(rect_layout.padding_bottom.map(|prop| prop.map_property(|b| b.0))),
            },
            border: Offsets {
                left: translate_dimension(rect_layout.border_left_width.map(|prop| prop.map_property(|l| l.0))),
                right: translate_dimension(rect_layout.border_right_width.map(|prop| prop.map_property(|r| r.0))),
                top: translate_dimension(rect_layout.border_top_width.map(|prop| prop.map_property(|t| t.0))),
                bottom: translate_dimension(rect_layout.border_bottom_width.map(|prop| prop.map_property(|b| b.0))),
            },
            flex_grow: rect_layout.flex_grow.unwrap_or_default().get_property_or_default().unwrap_or_default().0.get(),
            flex_shrink: rect_layout.flex_shrink.unwrap_or_default().get_property_or_default().unwrap_or_default().0.get(),
            size: Size {
                width: translate_dimension(rect_layout.width.map(|prop| prop.map_property(|l| l.0))),
                height: translate_dimension(rect_layout.height.map(|prop| prop.map_property(|l| l.0))),
            },
            min_size: Size {
                width: translate_dimension(rect_layout.min_width.map(|prop| prop.map_property(|l| l.0))),
                height: translate_dimension(rect_layout.min_height.map(|prop| prop.map_property(|l| l.0))),
            },
            max_size: Size {
                width: translate_dimension(rect_layout.max_width.map(|prop| prop.map_property(|l| l.0))),
                height: translate_dimension(rect_layout.max_height.map(|prop| prop.map_property(|l| l.0))),
            },
            align_self: AlignSelf::Auto, // todo!
            flex_basis: Dimension::Auto, // todo!
            aspect_ratio: Number::Undefined,
            font_size_px: rect_style.font_size.and_then(|fs| fs.get_property_owned()).unwrap_or(DEFAULT_FONT_SIZE).0,
            line_height: rect_style.line_height.and_then(|lh| lh.map_property(|lh| lh.0).get_property_owned()).map(|lh| lh.get()),
            letter_spacing: rect_style.letter_spacing.and_then(|ls| ls.map_property(|ls| ls.0).get_property_owned()),
            word_spacing: rect_style.word_spacing.and_then(|ws| ws.map_property(|ws| ws.0).get_property_owned()),
            tab_width: rect_style.tab_width.and_then(|tw| tw.map_property(|tw| tw.0).get_property_owned()).map(|tw| tw.get()),
        }
    }
}

/// Parameters that apply to a single rectangle / div node
#[derive(Copy, Clone)]
struct LayoutRectParams<'a, T: 'a> {
    epoch: Epoch,
    rect_idx: NodeId,
    html_node: &'a NodeType<T>,
    window_size: WindowSize,
}

#[derive(Debug, Clone, PartialEq)]
struct ContentGroup {
    /// The parent of the current node group, i.e. either the root node (0)
    /// or the last positioned node ()
    root: NodeId,
    /// Node ids in order of drawing
    children: Vec<ContentGroup>,
}

fn determine_rendering_order<'a>(
    node_hierarchy: &NodeHierarchy,
    rectangles: &NodeDataContainer<DisplayRectangle<'a>>,
) -> ContentGroup {

    let children_sorted: BTreeMap<NodeId, Vec<NodeId>> = node_hierarchy
        .get_parents_sorted_by_depth()
        .into_iter()
        .map(|(_, parent_id)| (parent_id, sort_children_by_position(parent_id, node_hierarchy, rectangles)))
        .collect();

    let mut root_content_group = ContentGroup { root: NodeId::ZERO, children: Vec::new() };
    fill_content_group_children(&mut root_content_group, &children_sorted);
    root_content_group
}

fn fill_content_group_children(group: &mut ContentGroup, children_sorted: &BTreeMap<NodeId, Vec<NodeId>>) {
    if let Some(c) = children_sorted.get(&group.root) { // returns None for leaf nodes
        group.children = c
            .iter()
            .map(|child| ContentGroup { root: *child, children: Vec::new() })
            .collect();

        for c in &mut group.children {
            fill_content_group_children(c, children_sorted);
        }
    }
}

fn sort_children_by_position<'a>(
    parent: NodeId,
    node_hierarchy: &NodeHierarchy,
    rectangles: &NodeDataContainer<DisplayRectangle<'a>>
) -> Vec<NodeId> {
    use azul_css::LayoutPosition::*;

    let mut not_absolute_children = parent
        .children(node_hierarchy)
        .filter(|id| rectangles[*id].layout.position.and_then(|p| p.get_property_or_default()).unwrap_or_default() != Absolute)
        .collect::<Vec<NodeId>>();

    let mut absolute_children = parent
        .children(node_hierarchy)
        .filter(|id| rectangles[*id].layout.position.and_then(|p| p.get_property_or_default()).unwrap_or_default() == Absolute)
        .collect::<Vec<NodeId>>();

    // Append the position:absolute children after the regular children
    not_absolute_children.append(&mut absolute_children);
    not_absolute_children
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
fn get_nodes_that_need_scroll_clip<'a, T: 'a>(
    node_hierarchy: &NodeHierarchy,
    display_list_rects: &NodeDataContainer<DisplayRectangle<'a>>,
    dom_rects: &NodeDataContainer<NodeData<T>>,
    layouted_rects: &NodeDataContainer<PositionedRectangle>,
    parents: &[(usize, NodeId)],
    pipeline_id: PipelineId,
) -> ScrolledNodes {

    use azul_css::Overflow;

    let mut nodes = BTreeMap::new();
    let mut tags_to_node_ids = BTreeMap::new();

    for (_, parent) in parents {

        let parent_rect = &layouted_rects[*parent];

        let children_scroll_rect = match parent_rect.bounds.get_scroll_rect(parent.children(&node_hierarchy).map(|child_id| layouted_rects[child_id].bounds)) {
            None => continue,
            Some(sum) => sum,
        };

        // Check if the scroll rect overflows the parent bounds
        if contains_rect_rounded(&parent_rect.bounds, children_scroll_rect) {
            continue;
        }

        // If the overflow isn't "scroll", then there doesn't need to be a scroll frame
        if parent_rect.overflow == Overflow::Visible || parent_rect.overflow == Overflow::Hidden {
            continue;
        }

        let parent_dom_hash = dom_rects[*parent].calculate_node_data_hash();

        // Create an external scroll id. This id is required to preserve its
        // scroll state accross multiple frames.
        let parent_external_scroll_id  = ExternalScrollId(parent_dom_hash.0, pipeline_id);

        // Create a unique scroll tag for hit-testing
        let scroll_tag_id = match display_list_rects.get(*parent).and_then(|node| node.tag) {
            Some(existing_tag) => ScrollTagId(existing_tag),
            None => ScrollTagId::new(),
        };

        tags_to_node_ids.insert(scroll_tag_id, *parent);
        nodes.insert(*parent, OverflowingScrollNode {
            child_rect: children_scroll_rect,
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

/// NOTE: This function assumes that the UiDescription has an initialized arena
///
/// This only looks at the user-facing styles of the `UiDescription`, not the actual
/// layout. The layout is done only in the `into_display_list_builder` step.
pub(crate) fn display_list_from_ui_description<'a, T>(
    ui_description: &'a UiDescription<T>,
    ui_state: &UiState<T>
) -> DisplayList<'a, T> {

    let arena = &ui_description.ui_descr_arena;

    let mut override_warnings = Vec::new();

    let display_rect_arena = arena.node_data.transform(|_, node_id| {
        let style = &ui_description.styled_nodes[node_id];
        let tag = ui_state.node_ids_to_tag_ids.get(&node_id).map(|tag| *tag);
        let mut rect = DisplayRectangle::new(tag, style);
        override_warnings.append(&mut populate_css_properties(&mut rect, node_id, &ui_description.dynamic_css_overrides));
        rect
    });

    #[cfg(feature = "logging")] {
        for warning in override_warnings {
            error!(
                "Cannot override {} with {:?}",
                warning.default.get_type(), warning.overridden_property,
            )
        }
    }

    DisplayList {
        ui_descr: ui_description,
        rectangles: display_rect_arena,
    }
}

pub(crate) struct CachedDisplayListResult {
    pub cached_display_list: CachedDisplayList,
    pub scrollable_nodes: BTreeMap<DomId, ScrolledNodes>,
    pub layout_result: BTreeMap<DomId, LayoutResult>,
    pub image_resource_updates: BTreeMap<DomId, Vec<(ImageId, AddImageMsg)>>,
}

/// Inserts and solves the top-level DOM (i.e. the DOM with the ID 0)
pub(crate) fn display_list_to_cached_display_list<'a, T, U: FontImageApi>(
    display_list: DisplayList<'a, T> ,
    app_data_access: &mut T,
    window: &mut Window<T>,
    fake_window: &mut FakeWindow<T>,
    app_resources: &mut AppResources,
    render_api: &mut U,
) -> CachedDisplayListResult {

    use app_resources::add_fonts_and_images;

    let arena = &display_list.ui_descr.ui_descr_arena;
    let node_hierarchy = &arena.node_layout;
    let node_data = &arena.node_data;
    let root_dom_id = display_list.ui_descr.dom_id.clone();

    // Scan the styled DOM for image and font keys.
    //
    // The problem is that we need to scan all DOMs for image and font keys and insert them
    // before the layout() step - however, can't call IFrameCallbacks upfront, because each
    // IFrameCallback needs to know its size (so it has to be invoked after the layout() step).
    // So, this process needs to follow an order like:
    //
    // - For each DOM to render:
    //      - Create a DOM ID
    //      - Style the DOM according to the stylesheet
    //      - Scan all the font keys and image keys
    //      - Insert the new font keys and image keys into the render API
    //      - Scan all IFrameCallbacks, generate the DomID for each callback
    //      - Repeat while number_of_iframe_callbacks != 0
    add_fonts_and_images(app_resources, render_api, &display_list);

    let layout_result = do_the_layout(
        node_hierarchy,
        node_data,
        &display_list.rectangles,
        &*app_resources,
        LayoutRect {
            origin: LayoutPoint::new(0.0, 0.0),
            size: LayoutSize::new(window.state.size.dimensions.width, window.state.size.dimensions.height),
        },
    );

    let rects_in_rendering_order = determine_rendering_order(
        node_hierarchy,
        &display_list.rectangles,
    );

    let scrollable_nodes = get_nodes_that_need_scroll_clip(
        node_hierarchy, &display_list.rectangles, node_data, &layout_result.rects,
        &layout_result.node_depths, window.internal.pipeline_id
    );

    let mut scrollable_nodes_map = BTreeMap::new();
    scrollable_nodes_map.insert(root_dom_id.clone(), scrollable_nodes);

    let mut layout_result_map = BTreeMap::new();
    layout_result_map.insert(root_dom_id.clone(), layout_result);

    let mut image_resource_updates = BTreeMap::new();

    let root_node = push_rectangles_into_displaylist(
        window.internal.epoch,
        window.state.size,
        rects_in_rendering_order,
        &DisplayListParametersRef {
            dom_id: root_dom_id,
            pipeline_id: window.internal.pipeline_id,
            node_hierarchy,
            node_data,
            display_rectangle_arena: &display_list.rectangles,
            css: &window.css,
        },
        &mut DisplayListParametersMut {
            app_data: app_data_access,
            app_resources,
            fake_window,
            image_resource_updates: &mut image_resource_updates,
            render_api,
            layout_result: &mut layout_result_map,
            scrollable_nodes: &mut scrollable_nodes_map,
        },
    );

    let cached_display_list = CachedDisplayList { root: root_node };

    CachedDisplayListResult {
        cached_display_list,
        scrollable_nodes: scrollable_nodes_map,
        layout_result: layout_result_map,
        image_resource_updates,
    }
}

fn push_rectangles_into_displaylist<'a, 'b, 'c, 'd, 'e, 'f, T, U: FontImageApi>(
    epoch: Epoch,
    window_size: WindowSize,
    root_content_group: ContentGroup,
    referenced_content: &DisplayListParametersRef<'a,'b,'c,'d, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T, U>,
) -> DisplayListMsg {

    let rectangle = LayoutRectParams {
        epoch,
        rect_idx: root_content_group.root,
        html_node: referenced_content.node_data[root_content_group.root].get_node_type(),
        window_size,
    };

    let mut content = displaylist_handle_rect(
        &rectangle,
        referenced_content,
        referenced_mutable_content,
    );

    let children = root_content_group.children.into_iter().map(|child_content_group| {
        push_rectangles_into_displaylist(
            epoch,
            window_size,
            child_content_group,
            referenced_content,
            referenced_mutable_content
        )
    }).collect();

    content.append_children(children);

    content
}

/// Push a single rectangle into the display list builder
fn displaylist_handle_rect<'a,'b,'c,'d,'e,'f, T, U: FontImageApi>(
    rectangle: &LayoutRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'b,'c,'d,'e, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T, U>,
) -> DisplayListMsg {

    let DisplayListParametersRef { display_rectangle_arena, dom_id, .. } = referenced_content;
    let LayoutRectParams { rect_idx, html_node, window_size, .. } = rectangle;

    let rect = &display_rectangle_arena[*rect_idx];
    let bounds = referenced_mutable_content.layout_result[dom_id].rects[*rect_idx].bounds;

    let display_list_rect_bounds = LayoutRect::new(
         LayoutPoint::new(bounds.origin.x, bounds.origin.y),
         LayoutSize::new(bounds.size.width, bounds.size.height),
    );

    let tag_id = rect.tag.map(|tag| (tag, 0)).or({
        referenced_mutable_content.scrollable_nodes[dom_id].overflowing_nodes
        .get(&rect_idx)
        .map(|scrolled| (scrolled.scroll_tag_id.0, 0))
    });

    let mut frame = DisplayListFrame {
        tag: tag_id,
        clip_rect: None,
        border_radius: StyleBorderRadius {
            top_left: rect.style.border_top_left_radius,
            top_right: rect.style.border_top_right_radius,
            bottom_left: rect.style.border_bottom_left_radius,
            bottom_right: rect.style.border_bottom_right_radius,
        },
        rect: display_list_rect_bounds,
        content: Vec::new(),
        children: Vec::new(),
    };

    if rect.style.has_box_shadow() {
        frame.content.push(LayoutRectContent::BoxShadow {
            shadow: StyleBoxShadow {
                left: rect.style.box_shadow_left,
                right: rect.style.box_shadow_right,
                top: rect.style.box_shadow_top,
                bottom: rect.style.box_shadow_bottom,
            },
            clip_mode: BoxShadowClipMode::Outset,
        });
    }

    // If the rect is hit-testing relevant, we need to push a rect anyway.
    // Otherwise the hit-testing gets confused
    if let Some(bg) = rect.style.background.as_ref().and_then(|br| br.get_property()) {

        use azul_css::{CssImageId, StyleBackgroundContent::*};
        use azul_core::display_list::RectBackground;

        fn get_image_info(app_resources: &AppResources, style_image_id: &CssImageId) -> Option<RectBackground> {
            let image_id = app_resources.get_css_image_id(&style_image_id.0)?;
            let image_info = app_resources.get_image_info(image_id)?;
            Some(RectBackground::Image(*image_info))
        }

        let background_content = match bg {
            LinearGradient(lg) => Some(RectBackground::LinearGradient(lg.clone())),
            RadialGradient(rg) => Some(RectBackground::RadialGradient(rg.clone())),
            Image(style_image_id) => get_image_info(referenced_mutable_content.app_resources, style_image_id),
            Color(c) => Some(RectBackground::Color(*c)),
        };

        if let Some(background_content) = background_content {
            frame.content.push(LayoutRectContent::Background {
                content: background_content,
                size: rect.style.background_size.and_then(|bs| bs.get_property().cloned()),
                offset: rect.style.background_position.and_then(|bs| bs.get_property().cloned()),
                repeat: rect.style.background_repeat.and_then(|bs| bs.get_property().cloned()),
            });
        }
    }

    match html_node {
        Div => { },
        Text(_) | Label(_) => {
            if let Some(layouted_glyphs) = referenced_mutable_content.layout_result[dom_id].layouted_glyph_cache.get(&rect_idx).cloned() {

                use azul_core::ui_solver::DEFAULT_FONT_COLOR;
                use wr_translate::wr_translate_logical_size;

                let text_color = rect.style.text_color.and_then(|tc| tc.get_property().cloned()).unwrap_or(DEFAULT_FONT_COLOR).0;
                let positioned_words = &referenced_mutable_content.layout_result[dom_id].positioned_word_cache[&rect_idx];
                let font_instance_key = positioned_words.1;

                frame.content.push(get_text(
                    display_list_rect_bounds,
                    &referenced_mutable_content.layout_result[dom_id].rects[*rect_idx].padding,
                    wr_translate_logical_size(window_size.dimensions),
                    layouted_glyphs,
                    font_instance_key,
                    text_color,
                    &rect.layout,
                ));
            }
        },
        Image(image_id) => {
            if let Some(image_info) = referenced_mutable_content.app_resources.get_image_info(image_id) {
                frame.content.push(LayoutRectContent::Image {
                    size: LayoutSize::new(bounds.size.width, bounds.size.height),
                    offset: LayoutPoint::new(0.0, 0.0),
                    image_rendering: ImageRendering::Auto,
                    alpha_type: AlphaType::PremultipliedAlpha,
                    image_key: image_info.key,
                    background_color: ColorU::WHITE,
                });
            }
        },
        GlTexture(callback) => {
            frame.content.push(call_opengl_callback(callback, bounds, dom_id.clone(), rectangle, referenced_mutable_content));
        },
        IFrame(callback) => {
            let parent = Some((dom_id.clone(), *rect_idx));
            frame.children.push(call_iframe_callback(callback, bounds, rectangle, referenced_content, referenced_mutable_content, parent));
        },
    };

    if rect.style.has_border() {
        frame.content.push(LayoutRectContent::Border {
            widths: StyleBorderWidths {
                top: rect.layout.border_top_width,
                left: rect.layout.border_left_width,
                bottom: rect.layout.border_bottom_width,
                right: rect.layout.border_right_width,
            },
            colors: StyleBorderColors {
                top: rect.style.border_top_color,
                left: rect.style.border_left_color,
                bottom: rect.style.border_bottom_color,
                right: rect.style.border_right_color,
            },
            styles: StyleBorderStyles {
                top: rect.style.border_top_style,
                left: rect.style.border_left_style,
                bottom: rect.style.border_bottom_style,
                right: rect.style.border_right_style,
            },
        });
    }

    if rect.style.has_box_shadow() {
        frame.content.push(LayoutRectContent::BoxShadow {
            shadow: StyleBoxShadow {
                left: rect.style.box_shadow_left,
                right: rect.style.box_shadow_right,
                top: rect.style.box_shadow_top,
                bottom: rect.style.box_shadow_bottom,
            },
            clip_mode: BoxShadowClipMode::Inset,
        });
    }

    match referenced_mutable_content.scrollable_nodes[dom_id].overflowing_nodes.get(&rect_idx) {
        Some(scroll_node) => DisplayListMsg::ScrollFrame(DisplayListScrollFrame {
            content_rect: scroll_node.child_rect,
            scroll_id: scroll_node.parent_external_scroll_id,
            scroll_tag: scroll_node.scroll_tag_id,
            frame,
        }),
        None => DisplayListMsg::Frame(frame),
    }
}

#[inline]
fn call_opengl_callback<'a,'b,'c,'d,'e,'f, T, U: FontImageApi>(
    (texture_callback, texture_stack_ptr): &(GlCallback<T>, StackCheckedPointer<T>),
    bounds: LayoutRect,
    dom_id: DomId,
    rectangle: &LayoutRectParams<'a, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T, U>,
) -> LayoutRectContent {

    use gleam::gl;
    use {
        compositor::get_active_gl_textures,
        wr_translate::{hidpi_rect_from_bounds, wr_translate_image_key, wr_translate_image_descriptor},
        app_resources::ImageInfo,
    };
    use azul_core::{
        callbacks::GlCallbackInfoUnchecked,
        display_list::RectBackground,
        app_resources::{ImageDescriptor, RawImageFormat}
    };

    let bounds = hidpi_rect_from_bounds(
        bounds,
        rectangle.window_size.hidpi_factor,
        rectangle.window_size.winit_hidpi_factor
    );

    let texture = {

        let tex = (texture_callback.0)(GlCallbackInfoUnchecked {
            ptr: *texture_stack_ptr,
            layout_info: LayoutInfo {
                window: &mut *referenced_mutable_content.fake_window,
                resources: &referenced_mutable_content.app_resources,
            },
            bounds,
        });

        // Reset the framebuffer and SRGB color target to 0
        let gl_context = &*referenced_mutable_content.fake_window.gl_context;

        gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
        gl_context.disable(gl::FRAMEBUFFER_SRGB);
        gl_context.disable(gl::MULTISAMPLE);

        tex
    };

    let texture = match texture {
        Some(s) => s,
        None => return LayoutRectContent::Background {
            content: RectBackground::Color(ColorU::TRANSPARENT),
            size: None,
            offset: None,
            repeat: None,
        },
    };

    let opaque = false;
    // The texture gets mapped 1:1 onto the display, so there is no need for mipmaps
    let allow_mipmaps = false;

    let texture_width = texture.size.width;
    let texture_height = texture.size.height;

    // Note: The ImageDescriptor has no effect on how large the image appears on-screen
    let descriptor = ImageDescriptor {
        format: RawImageFormat::RGBA8,
        dimensions: (texture.size.width as usize, texture.size.height as usize),
        stride: None,
        offset: 0,
        is_opaque: opaque,
        allow_mipmaps,
    };
    let key = referenced_mutable_content.render_api.new_image_key();
    let external_image_id = ExternalImageId(new_opengl_texture_id() as u64);

    get_active_gl_textures()
        .entry(rectangle.epoch).or_insert_with(|| FastHashMap::default())
        .insert(external_image_id, texture);

    let add_img_msg = AddImageMsg(
        AddImage {
            key: wr_translate_image_key(key),
            descriptor: wr_translate_image_descriptor(descriptor),
            data: ImageData::External(ExternalImageData {
                id: external_image_id,
                channel_index: 0,
                image_type: ExternalImageType::TextureHandle(TextureTarget::Default),
            }),
            tiling: None,
        },
        ImageInfo { key, descriptor }
    );

    referenced_mutable_content.image_resource_updates
        .entry(dom_id)
        .or_insert_with(|| Vec::new())
        .push((ImageId::new(), add_img_msg));

    LayoutRectContent::Image {
        size: LayoutSize::new(texture_width, texture_height),
        offset: LayoutPoint::new(0.0, 0.0),
        image_rendering: ImageRendering::Auto,
        alpha_type: AlphaType::Alpha,
        image_key: key,
        background_color: ColorU::WHITE,
    }
}

#[inline]
fn call_iframe_callback<'a,'b,'c,'d,'e, T, U: FontImageApi>(
    (iframe_callback, iframe_pointer): &(IFrameCallback<T>, StackCheckedPointer<T>),
    rect: LayoutRect,
    rectangle: &LayoutRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'a,'b,'c,'d, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'e, T, U>,
    parent_dom_id: Option<(DomId, NodeId)>,
) -> DisplayListMsg {

    use app_resources;
    use ui_state::ui_state_from_dom;
    use wr_translate::hidpi_rect_from_bounds;
    use azul_core::callbacks::IFrameCallbackInfoUnchecked;

    let bounds = hidpi_rect_from_bounds(
        rect,
        rectangle.window_size.hidpi_factor,
        rectangle.window_size.hidpi_factor
    );

    let new_dom = {
        let iframe_info = IFrameCallbackInfoUnchecked {
            ptr: *iframe_pointer,
            layout_info: LayoutInfo {
                window: referenced_mutable_content.fake_window,
                resources: &referenced_mutable_content.app_resources,
            },
            bounds,
        };

        (iframe_callback.0)(iframe_info)
    };

    let new_dom = match new_dom {
        Some(s) => s,
        None => return DisplayListMsg::Frame(DisplayListFrame {
            tag: None,
            clip_rect: None,
            rect,
            border_radius: StyleBorderRadius::default(),
            content: vec![],
            children: vec![],
        }),
    };

    // TODO: Right now, no focusing, hovering or :active allowed in iframes!
    let is_mouse_down = false;
    let mut focused_node = None;
    let mut focus_target = None;
    let hovered_nodes = BTreeMap::new();

    let mut ui_state = ui_state_from_dom(new_dom, parent_dom_id);
    let ui_description = UiDescription::<T>::match_css_to_dom(
        &mut ui_state,
        &referenced_content.css,
        &mut focused_node,
        &mut focus_target,
        &hovered_nodes,
        is_mouse_down,
    );

    let iframe_dom_id = ui_description.dom_id.clone();

    let display_list = display_list_from_ui_description(&ui_description, &ui_state);

    app_resources::add_fonts_and_images(
        referenced_mutable_content.app_resources,
        referenced_mutable_content.render_api,
        &display_list
    );

    let arena = &ui_description.ui_descr_arena;
    let node_hierarchy = &arena.node_layout;
    let node_data = &arena.node_data;

    // Insert the DOM into the solver so we can solve the layout of the rectangles
    let layout_result_iframe = do_the_layout(
        &node_hierarchy,
        &node_data,
        &display_list.rectangles,
        &*referenced_mutable_content.app_resources,
        rect,
    );

    let scrollable_nodes_iframe = get_nodes_that_need_scroll_clip(
        node_hierarchy, &display_list.rectangles, node_data, &layout_result_iframe.rects,
        &layout_result_iframe.node_depths, referenced_content.pipeline_id
    );

    let rects_in_rendering_order = determine_rendering_order(
        node_hierarchy,
        &display_list.rectangles,
    );

    referenced_mutable_content.scrollable_nodes.insert(iframe_dom_id.clone(), scrollable_nodes_iframe);
    referenced_mutable_content.layout_result.insert(iframe_dom_id.clone(), layout_result_iframe);

    let referenced_content = DisplayListParametersRef {
        // Important: Need to update the ui description,
        // otherwise this function would be endlessly recurse
        node_hierarchy,
        node_data,
        display_rectangle_arena: &display_list.rectangles,
        dom_id: iframe_dom_id,
        .. *referenced_content
    };

    push_rectangles_into_displaylist(
        rectangle.epoch,
        rectangle.window_size,
        rects_in_rendering_order,
        &referenced_content,
        referenced_mutable_content
    )
}

fn get_text(
    bounds: LayoutRect,
    padding: &ResolvedOffsets,
    root_window_size: LayoutSize,
    layouted_glyphs: LayoutedGlyphs,
    font_instance_key: FontInstanceKey,
    font_color: ColorU,
    rect_layout: &RectLayout,
) -> LayoutRectContent {

    let overflow_horizontal_visible = rect_layout.is_horizontal_overflow_visible();
    let overflow_vertical_visible = rect_layout.is_horizontal_overflow_visible();

    let padding_clip_bounds = subtract_padding(&bounds, padding);

    // Adjust the bounds by the padding, depending on the overflow:visible parameter
    let text_clip_rect = match (overflow_horizontal_visible, overflow_vertical_visible) {
        (true, true) => None,
        (false, false) => Some(padding_clip_bounds),
        (true, false) => {
            // Horizontally visible, vertically cut
            Some(LayoutRect {
                origin: bounds.origin,
                size: LayoutSize::new(root_window_size.width, padding_clip_bounds.size.height),
            })
        },
        (false, true) => {
            // Vertically visible, horizontally cut
            Some(LayoutRect {
                origin: bounds.origin,
                size: LayoutSize::new(padding_clip_bounds.size.width, root_window_size.height),
            })
        },
    };

    LayoutRectContent::Text {
        glyphs: layouted_glyphs.glyphs,
        font_instance_key,
        color: font_color,
        glyph_options: None,
        clip: text_clip_rect,
    }
}

/// Subtracts the padding from the bounds, returning the new bounds
///
/// Warning: The resulting rectangle may have negative width or height
fn subtract_padding(bounds: &LayoutRect, padding: &ResolvedOffsets) -> LayoutRect {

    let mut new_bounds = *bounds;

    new_bounds.origin.x += padding.left;
    new_bounds.size.width -= padding.right + padding.left;
    new_bounds.origin.y += padding.top;
    new_bounds.size.height -= padding.top + padding.bottom;

    new_bounds
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OverrideWarning {
    pub default: CssProperty,
    pub overridden_property: CssProperty,
}

/// Populate the style properties of the `DisplayRectangle`, apply static / dynamic properties
fn populate_css_properties(
    rect: &mut DisplayRectangle,
    node_id: NodeId,
    css_overrides: &BTreeMap<NodeId, FastHashMap<DomString, CssProperty>>,
) -> Vec<OverrideWarning> {

    use azul_css::CssDeclaration::*;
    use std::mem;

    let rect_style = &mut rect.style;
    let rect_layout = &mut rect.layout;
    let css_constraints = &rect.styled_node.css_constraints;

   css_constraints
    .values()
    .filter_map(|constraint| match constraint {
        Static(static_property) => {
            apply_style_property(rect_style, rect_layout, static_property);
            None
        },
        Dynamic(dynamic_property) => {
            let overridden_property = css_overrides.get(&node_id).and_then(|overrides| overrides.get(&dynamic_property.dynamic_id.clone().into()))?;

            // Apply the property default if the discriminant of the two types matches
            if mem::discriminant(overridden_property) == mem::discriminant(&dynamic_property.default_value) {
                apply_style_property(rect_style, rect_layout, overridden_property);
                None
            } else {
                Some(OverrideWarning {
                    default: dynamic_property.default_value.clone(),
                    overridden_property: overridden_property.clone(),
                })
            }
        },
    })
    .collect()
}

fn apply_style_property(style: &mut RectStyle, layout: &mut RectLayout, property: &CssProperty) {

    use azul_css::CssProperty::*;

    match property {

        Display(d)                      => layout.display = Some(*d),
        Float(f)                        => layout.float = Some(*f),
        BoxSizing(bs)                   => layout.box_sizing = Some(*bs),

        TextColor(c)                    => style.text_color = Some(*c),
        FontSize(fs)                    => style.font_size = Some(*fs),
        FontFamily(ff)                  => style.font_family = Some(ff.clone()),
        TextAlign(ta)                   => style.text_align = Some(*ta),

        LetterSpacing(ls)               => style.letter_spacing = Some(*ls),
        LineHeight(lh)                  => style.line_height = Some(*lh),
        WordSpacing(ws)                 => style.word_spacing = Some(*ws),
        TabWidth(tw)                    => style.tab_width = Some(*tw),
        Cursor(c)                       => style.cursor = Some(*c),

        Width(w)                        => layout.width = Some(*w),
        Height(h)                       => layout.height = Some(*h),
        MinWidth(mw)                    => layout.min_width = Some(*mw),
        MinHeight(mh)                   => layout.min_height = Some(*mh),
        MaxWidth(mw)                    => layout.max_width = Some(*mw),
        MaxHeight(mh)                   => layout.max_height = Some(*mh),

        Position(p)                     => layout.position = Some(*p),
        Top(t)                          => layout.top = Some(*t),
        Bottom(b)                       => layout.bottom = Some(*b),
        Right(r)                        => layout.right = Some(*r),
        Left(l)                         => layout.left = Some(*l),

        FlexWrap(fw)                    => layout.wrap = Some(*fw),
        FlexDirection(fd)               => layout.direction = Some(*fd),
        FlexGrow(fg)                    => layout.flex_grow = Some(*fg),
        FlexShrink(fs)                  => layout.flex_shrink = Some(*fs),
        JustifyContent(jc)              => layout.justify_content = Some(*jc),
        AlignItems(ai)                  => layout.align_items = Some(*ai),
        AlignContent(ac)                => layout.align_content = Some(*ac),

        BackgroundContent(bc)           => style.background = Some(bc.clone()),
        BackgroundPosition(bp)          => style.background_position = Some(*bp),
        BackgroundSize(bs)              => style.background_size = Some(*bs),
        BackgroundRepeat(br)            => style.background_repeat = Some(*br),

        OverflowX(ox)                   => layout.overflow_x = Some(*ox),
        OverflowY(oy)                   => layout.overflow_y = Some(*oy),

        PaddingTop(pt)                  => layout.padding_top = Some(*pt),
        PaddingLeft(pl)                 => layout.padding_left = Some(*pl),
        PaddingRight(pr)                => layout.padding_right = Some(*pr),
        PaddingBottom(pb)               => layout.padding_bottom = Some(*pb),

        MarginTop(mt)                   => layout.margin_top = Some(*mt),
        MarginLeft(ml)                  => layout.margin_left = Some(*ml),
        MarginRight(mr)                 => layout.margin_right = Some(*mr),
        MarginBottom(mb)                => layout.margin_bottom = Some(*mb),

        BorderTopLeftRadius(btl)        => style.border_top_left_radius = Some(*btl),
        BorderTopRightRadius(btr)       => style.border_top_right_radius = Some(*btr),
        BorderBottomLeftRadius(bbl)     => style.border_bottom_left_radius = Some(*bbl),
        BorderBottomRightRadius(bbr)    => style.border_bottom_right_radius = Some(*bbr),

        BorderTopColor(btc)             => style.border_top_color = Some(*btc),
        BorderRightColor(brc)           => style.border_right_color = Some(*brc),
        BorderLeftColor(blc)            => style.border_left_color = Some(*blc),
        BorderBottomColor(bbc)          => style.border_bottom_color = Some(*bbc),

        BorderTopStyle(bts)             => style.border_top_style = Some(*bts),
        BorderRightStyle(brs)           => style.border_right_style = Some(*brs),
        BorderLeftStyle(bls)            => style.border_left_style = Some(*bls),
        BorderBottomStyle(bbs)          => style.border_bottom_style = Some(*bbs),

        BorderTopWidth(btw)             => layout.border_top_width = Some(*btw),
        BorderRightWidth(brw)           => layout.border_right_width = Some(*brw),
        BorderLeftWidth(blw)            => layout.border_left_width = Some(*blw),
        BorderBottomWidth(bbw)          => layout.border_bottom_width = Some(*bbw),

        BoxShadowLeft(bsl)              => style.box_shadow_left = Some(*bsl),
        BoxShadowRight(bsr)             => style.box_shadow_right = Some(*bsr),
        BoxShadowTop(bst)               => style.box_shadow_top = Some(*bst),
        BoxShadowBottom(bsb)            => style.box_shadow_bottom = Some(*bsb),
    }
}

#[test]
fn test_overflow_parsing() {
    use prelude::Overflow;

    let layout1 = RectLayout::default();

    // The default for overflowing is overflow: auto, which clips
    // children, so this should evaluate to true by default
    assert_eq!(node_needs_to_clip_children(&layout1), true);

    let layout2 = RectLayout {
        overflow_x: Some(CssPropertyValue::Exact(Overflow::Visible)),
        overflow_y: Some(CssPropertyValue::Exact(Overflow::Visible)),
        .. Default::default()
    };
    assert_eq!(node_needs_to_clip_children(&layout2), false);

    let layout3 = RectLayout {
        overflow_x: Some(CssPropertyValue::Exact(Overflow::Hidden)),
        overflow_y: Some(CssPropertyValue::Exact(Overflow::Hidden)),
        .. Default::default()
    };
    assert_eq!(node_needs_to_clip_children(&layout3), true);
}