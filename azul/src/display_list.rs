#![allow(unused_variables)]

use std::{
    collections::BTreeMap,
};
use euclid::{TypedRect, TypedSize2D};
use webrender::api::{
    LayoutPixel, DisplayListBuilder, PrimitiveInfo, GradientStop,
    Epoch, ImageData, ImageDescriptor, ResourceUpdate, AddImage, ClipMode,
    LayoutPoint, LayoutSize, LayoutRect, ExternalScrollId,
    ComplexClipRegion, LayoutPrimitiveInfo, ExternalImageId,
    ExternalImageData, ImageFormat, ExternalImageType, TextureTarget, RenderApi,
};
use azul_css::{
    Css, LayoutPosition,CssProperty, LayoutOverflow, ColorU,
    StyleBorderRadius, LayoutMargin, LayoutPadding, BoxShadowClipMode,
    StyleTextColor, StyleBackground, StyleBoxShadow,
    StyleBackgroundSize, StyleBackgroundRepeat, StyleBorder,
    RectStyle, RectLayout, ColorU as StyleColorU, DynamicCssPropertyDefault,
};
use {
    FastHashMap,
    app_resources::AppResources,
    callbacks::{IFrameCallback, GlTextureCallback, StackCheckedPointer},
    ui_state::UiState,
    ui_description::{UiDescription, StyledNode},
    id_tree::{NodeDataContainer, NodeId, NodeHierarchy},
    dom::{
        NodeData, ScrollTagId, DomHash, DomString,
        NodeType::{self, Div, Text, Image, GlTexture, IFrame, Label},
    },
    ui_solver::{do_the_layout, LayoutResult, PositionedRectangle},
    app_resources::ImageId,
    compositor::new_opengl_texture_id,
    window::{Window, WindowSize, FakeWindow, ScrollStates},
    callbacks::LayoutInfo,
    text_layout::LayoutedGlyphs,
};
use azul_core::{
    callbacks::PipelineId,
    window::{LogicalSize, LogicalPosition},
    app_resources::FontInstanceKey,
    display_list::{
        CachedDisplayList, DisplayListMsg, DisplayListRect, DisplayListRectContent,
        ImageRendering, AlphaType, DisplayListFrame,
    },
};

const DEFAULT_FONT_COLOR: StyleTextColor = StyleTextColor(StyleColorU { r: 0, b: 0, g: 0, a: 255 });

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
#[derive(Copy, Clone)]
struct DisplayListParametersRef<'a, 'b, 'c, 'd, 'e, T: 'a> {
    pub node_data: &'a NodeDataContainer<NodeData<T>>,
    /// The CSS that should be applied to the DOM
    pub css: &'b Css,
    /// Laid out words and rectangles (contains info about content bounds and text layout)
    pub layout_result: &'c LayoutResult,
    /// Reference to the arena that contains all the styled rectangles
    pub display_rectangle_arena: &'d NodeDataContainer<DisplayRectangle<'d>>,
    /// Reference to the arena that contains the node hierarchy data, so
    /// that the node hierarchy can be re-used
    pub node_hierarchy: &'e NodeHierarchy,
    /// The current pipeline of the display list
    pub pipeline_id: PipelineId,
}

/// Same as `DisplayListParametersRef`, but for `&mut Something`
///
/// Note: The `'a` in the `'a + Layout` is technically not required.
/// Only rustc 1.28 requires this, more modern compiler versions insert it automatically.
struct DisplayListParametersMut<'a, T: 'a> {
    /// Needs to be present, because the dom_to_displaylist_builder
    /// could call (recursively) a sub-DOM function again, for example an OpenGL callback
    pub app_data: &'a mut T,
    /// The app resources, so that a sub-DOM / iframe can register fonts and images
    /// TODO: How to handle cleanup ???
    pub app_resources: &'a mut AppResources,
    /// If new fonts or other stuff are created, we need to tell WebRender about this
    pub resource_updates: &'a mut Vec<ResourceUpdate>,
    /// Window access, so that sub-items can register OpenGL textures
    pub fake_window: &'a mut FakeWindow<T>,
    /// The render API that fonts and images should be added onto.
    pub render_api: &'a mut RenderApi,
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

/// In order to render rectangles in the correct order, we have to group them together:
/// As long as there are no position:absolute items, items are inserted in a parents-then-child order
///
/// ```no_run,ignore
/// a
/// |- b
/// |- c
/// |  |- d
/// e
/// |- f
/// g
/// ```
/// is rendered in the order `a, b, c, d, e, f, g`. This is necessary for clipping and scrolling,
/// meaning that if there is an overflow:scroll element, all children of that element are clipped
/// within that group. This means, that the z-order is completely determined by the DOM hierarchy.
///
/// Trees with elements with `position:absolute` are more complex: The absolute items need
/// to be rendered completely on top of all other items, however, they still need to clip
/// and scroll properly.
///
/// ```no_run,ignore
/// a:relative
/// |- b
/// |- c:absolute
/// |  |- d
/// e
/// |- f
/// g
/// ```
///
/// will be rendered as: `a,b,e,f,g,c,d`, so that the `c,d` sub-DOM is on top of the rest
/// of the content. To support this, the content needs to be grouped: Whenever there is a
/// `position:absolute` encountered, the children are grouped into a new `ContentGroup`:
///
/// ```no_run,ignore
/// Group 1: [a, b, c, e, f, g]
/// Group 2: [c, d]
/// ```
/// Then the groups are simply rendered in-order: if there are multiple position:absolute
/// groups, this has the side effect of later groups drawing on top of earlier groups.
#[derive(Debug, Clone, PartialEq)]
struct ContentGroup {
    /// The parent of the current node group, i.e. either the root node (0)
    /// or the last positioned node ()
    root: RenderableNodeId,
    /// Depth of the root node in the DOM hierarchy
    root_depth: usize,
    /// Node ids in order of drawing
    node_ids: Vec<RenderableNodeId>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
struct RenderableNodeId {
    /// Whether the (hierarchical) children of this group need to be clipped (usually
    /// because the parent has an `overflow:hidden` property set).
    clip_children: bool,
    /// Whether the children overflow the parent (see `O`)
    scrolls_children: bool,
    /// The actual node ID of the content
    node_id: NodeId,
}

#[derive(Debug, Clone, PartialEq)]
struct ContentGroupOrder {
    groups: Vec<ContentGroup>,
}

#[derive(Default, Debug, Clone)]
pub(crate) struct ScrolledNodes {
    pub(crate) overflowing_nodes: BTreeMap<NodeId, OverflowingScrollNode>,
    pub(crate) tags_to_node_ids: BTreeMap<ScrollTagId, NodeId>,
}

#[derive(Debug, Clone)]
pub(crate) struct OverflowingScrollNode {
    pub(crate) parent_rect: PositionedRectangle,
    pub(crate) child_rect: LayoutRect,
    pub(crate) parent_external_scroll_id: ExternalScrollId,
    pub(crate) parent_dom_hash: DomHash,
    pub(crate) scroll_tag_id: ScrollTagId,
}

/// Parameters that apply to a single rectangle / div node
#[derive(Copy, Clone)]
pub(crate) struct DisplayListRectParams<'a, T: 'a> {
    pub epoch: Epoch,
    pub rect_idx: NodeId,
    pub html_node: &'a NodeType<T>,
    window_size: WindowSize,
}

fn determine_rendering_order<'a>(
    node_hierarchy: &NodeHierarchy,
    rectangles: &NodeDataContainer<DisplayRectangle<'a>>,
    layouted_rects: &NodeDataContainer<PositionedRectangle>,
) -> ContentGroupOrder
{
    let mut content_groups = Vec::new();

    determine_rendering_order_inner(
        node_hierarchy,
        rectangles,
        layouted_rects,
        0, // depth of this node
        NodeId::new(0),
        &mut content_groups
    );

    ContentGroupOrder { groups: content_groups }
}

fn determine_rendering_order_inner<'a>(
    node_hierarchy: &NodeHierarchy,
    rectangles: &NodeDataContainer<DisplayRectangle<'a>>,
    layouted_rects: &NodeDataContainer<PositionedRectangle>,
    // recursive parameters
    root_depth: usize,
    root_id: NodeId,
    content_groups: &mut Vec<ContentGroup>,
) {
    use id_tree::NodeEdge;

    let mut root_group = ContentGroup {
        root: RenderableNodeId {
            node_id: root_id,
            clip_children: node_needs_to_clip_children(&rectangles[root_id].layout),
            scrolls_children: false, // TODO
        },
        root_depth,
        node_ids: Vec::new(),
    };

    let mut absolute_node_ids = Vec::new();
    let mut depth = root_depth + 1;

    // Same as the traverse function, but allows us to skip items, returns the next element
    fn traverse_simple(root_id: NodeId, current_node: NodeEdge<NodeId>, node_hierarchy: &NodeHierarchy) -> Option<NodeEdge<NodeId>> {
        // returns the next item
        match current_node {
            NodeEdge::Start(current_node) => {
                match node_hierarchy[current_node].first_child {
                    Some(first_child) => Some(NodeEdge::Start(first_child)),
                    None => Some(NodeEdge::End(current_node.clone()))
                }
            }
            NodeEdge::End(current_node) => {
                if current_node == root_id {
                    None
                } else {
                    match node_hierarchy[current_node].next_sibling {
                        Some(next_sibling) => Some(NodeEdge::Start(next_sibling)),
                        None => node_hierarchy[current_node].parent.and_then(|parent| Some(NodeEdge::End(parent))),
                    }
                }
            }
        }
    }

    let mut current_node_edge = NodeEdge::Start(root_id);
    while let Some(next_node_id) = traverse_simple(root_id, current_node_edge.clone(), node_hierarchy) {
        let mut should_continue_loop = true;

        if next_node_id.clone().inner_value() != root_id {
            match next_node_id {
                NodeEdge::Start(node_id) => {
                    let rect_node = &rectangles[node_id];
                    let position = rect_node.layout.position.unwrap_or_default();
                    if position == LayoutPosition::Absolute {
                        // For now, ignore the node and put it aside for later
                        absolute_node_ids.push((depth, node_id));
                        // Skip this sub-tree and go straight to the next sibling
                        // Since the tree is positioned absolute, we'll worry about it later
                        current_node_edge = NodeEdge::End(node_id);
                        should_continue_loop = false;
                    } else {
                        // TODO: Overflow hidden in horizontal / vertical direction
                        let node_is_overflow_hidden = node_needs_to_clip_children(&rect_node.layout);
                        let node_needs_to_scroll_children = false; // TODO
                        root_group.node_ids.push(RenderableNodeId {
                            node_id,
                            clip_children: node_is_overflow_hidden,
                            scrolls_children: node_needs_to_scroll_children,
                        });
                    }

                    depth += 1;
                },
                NodeEdge::End(node_id) => {
                    depth -= 1;
                },
            }
        }

        if should_continue_loop {
            current_node_edge = next_node_id;
        }
    }

    content_groups.push(root_group);

    // Note: Currently reversed order, so that earlier absolute
    // items are drawn on top of later absolute items
    for (absolute_depth, absolute_node_id) in absolute_node_ids.into_iter().rev() {
        determine_rendering_order_inner(node_hierarchy, rectangles, layouted_rects, absolute_depth, absolute_node_id, content_groups);
    }
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

    use wr_translate;

    let mut nodes = BTreeMap::new();
    let mut tags_to_node_ids = BTreeMap::new();

    for (_, parent) in parents {

        let mut children_sum_rect = None;

        for child in parent.children(&node_hierarchy) {
            let old = children_sum_rect.unwrap_or(LayoutRect::zero());
            children_sum_rect = Some(old.union(&layouted_rects[child].bounds));
        }

        let children_sum_rect = match children_sum_rect {
            None => continue,
            Some(sum) => sum,
        };

        let parent_rect = layouted_rects.get(*parent).unwrap();

        if children_sum_rect.contains_rect(&parent_rect.bounds) {
            continue;
        }

        let parent_dom_hash = dom_rects[*parent].calculate_node_data_hash();

        // Create an external scroll id. This id is required to preserve its
        // scroll state accross multiple frames.
        let parent_external_scroll_id  = ExternalScrollId(
            parent_dom_hash.0, wr_translate::wr_translate_pipeline_id(pipeline_id)
        );

        // Create a unique scroll tag for hit-testing
        let scroll_tag_id = match display_list_rects.get(*parent).and_then(|node| node.tag) {
            Some(existing_tag) => ScrollTagId(existing_tag),
            None => ScrollTagId::new(),
        };

        tags_to_node_ids.insert(scroll_tag_id, *parent);
        nodes.insert(*parent, OverflowingScrollNode {
            parent_rect: parent_rect.clone(),
            child_rect: children_sum_rect,
            parent_external_scroll_id,
            parent_dom_hash,
            scroll_tag_id,
        });
    }

    ScrolledNodes { overflowing_nodes: nodes, tags_to_node_ids }
}

fn node_needs_to_clip_children(layout: &RectLayout) -> bool {
    let overflow = layout.overflow.unwrap_or_default();
    !overflow.is_horizontal_overflow_visible() ||
    !overflow.is_vertical_overflow_visible()
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

    let display_rect_arena = arena.node_data.transform(|node, node_id| {
        let style = &ui_description.styled_nodes[node_id];
        let tag = ui_state.node_ids_to_tag_ids.get(&node_id).map(|tag| *tag);
        let mut rect = DisplayRectangle::new(tag, style);
        populate_css_properties(&mut rect, node_id, &ui_description.dynamic_css_overrides);
        rect
    });

    DisplayList {
        ui_descr: ui_description,
        rectangles: display_rect_arena,
    }
}

/// Inserts and solves the top-level DOM (i.e. the DOM with the ID 0)
pub(crate) fn display_list_to_cached_display_list<'a, T>(
    display_list: DisplayList<'a, T> ,
    app_data_access: &mut T,
    window: &mut Window<T>,
    fake_window: &mut FakeWindow<T>,
    app_resources: &mut AppResources,
    render_api: &mut RenderApi,
) -> (CachedDisplayList, ScrolledNodes, LayoutResult) {

    use app_resources::add_fonts_and_images;

    let mut resource_updates = Vec::<ResourceUpdate>::new();

    let arena = &display_list.ui_descr.ui_descr_arena;
    let node_hierarchy = &arena.node_layout;
    let node_data = &arena.node_data;

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

    let window_size = window.state.size.get_reverse_logical_size();
    let layout_result = do_the_layout(
        node_hierarchy,
        node_data,
        &display_list.rectangles,
        &*app_resources,
        LayoutSize::new(window_size.width as f32, window_size.height as f32),
        LayoutPoint::new(0.0, 0.0),
    );

    let rects_in_rendering_order = determine_rendering_order(
        node_hierarchy,
        &display_list.rectangles,
        &layout_result.rects
    );

    let mut scrollable_nodes = get_nodes_that_need_scroll_clip(
        node_hierarchy, &display_list.rectangles, node_data, &layout_result.rects,
        &layout_result.node_depths, window.internal.pipeline_id
    );

    // Make sure unused scroll states are garbage collected.
    window.scroll_states.remove_unused_scroll_states();

    let root_node = push_rectangles_into_displaylist(
        window.internal.epoch,
        window.state.size,
        rects_in_rendering_order,
        &mut scrollable_nodes,
        &mut window.scroll_states,
        &DisplayListParametersRef {
            pipeline_id: window.internal.pipeline_id,
            node_hierarchy,
            node_data,
            display_rectangle_arena: &display_list.rectangles,
            css: &window.css,
            layout_result: &layout_result,
        },
        &mut DisplayListParametersMut {
            app_data: app_data_access,
            app_resources,
            fake_window,
            resource_updates: &mut resource_updates,
            render_api,
        },
    );

    let cached_display_list = CachedDisplayList {
        root: root_node,
        pipeline_id: window.internal.pipeline_id,

    };

    (cached_display_list, scrollable_nodes, layout_result)
}

fn push_rectangles_into_displaylist<'a, 'b, 'c, 'd, 'e, 'f, T>(
    epoch: Epoch,
    window_size: WindowSize,
    content_grouped_rectangles: ContentGroupOrder,
    scrollable_nodes: &mut ScrolledNodes,
    scroll_states: &mut ScrollStates,
    referenced_content: &DisplayListParametersRef<'a,'b,'c,'d,'e, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T>,
) -> DisplayListMsg {

    let root_children = content_grouped_rectangles.groups.into_iter().map(|content_group| {

        let rectangle = DisplayListRectParams {
            epoch,
            rect_idx: content_group.root.node_id,
            html_node: referenced_content.node_data[content_group.root.node_id].get_node_type(),
            window_size,
        };

        // TODO: overflow / scroll frames!
        let mut content: DisplayListMsg = DisplayListMsg::Frame(displaylist_handle_rect(
            scrollable_nodes,
            &rectangle,
            referenced_content,
            referenced_mutable_content,
        ));

        let children = content_group.node_ids.iter().map(|item| {

            let rectangle = DisplayListRectParams {
                epoch,
                rect_idx: item.node_id,
                html_node: referenced_content.node_data[item.node_id].get_node_type(),
                window_size,
            };

            // TODO: overflow / scroll frames!
            DisplayListMsg::Frame(displaylist_handle_rect(
                scrollable_nodes,
                &rectangle,
                referenced_content,
                referenced_mutable_content,
            ))
        }).collect::<Vec<DisplayListMsg>>();

        content.append_children(children);

        content
    }).collect();

    DisplayListMsg::Frame(DisplayListFrame {
        tag: None,
        clip_rect: None,
        rect: DisplayListRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: window_size.dimensions,
        },
        content: vec![DisplayListRectContent::Background {
            background_type: StyleBackground::Color(ColorU { r: 255, g: 255, b: 255, a: 255 }),
        }],
        children: root_children, // Vec<DisplayListMsg>
    })
}

fn get_clip_region<'a>(bounds: LayoutRect, rect: &DisplayRectangle<'a>) -> Option<ComplexClipRegion> {
    use wr_translate::wr_translate_border_radius;
    rect.style.border_radius.and_then(|border_radius| {
        Some(ComplexClipRegion {
            rect: bounds,
            radii: wr_translate_border_radius(border_radius.0).into(),
            mode: ClipMode::Clip,
        })
    })
}

/// Push a single rectangle into the display list builder
fn displaylist_handle_rect<'a,'b,'c,'d,'e,'f,'g, T>(
    scrollable_nodes: &mut ScrolledNodes,
    rectangle: &DisplayListRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'b,'c,'d,'e,'f, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'g, T>,
) -> DisplayListFrame {

    let DisplayListParametersRef {
        css, display_rectangle_arena,
        pipeline_id, node_hierarchy, node_data,
        layout_result,
    } = referenced_content;

    let DisplayListRectParams {
        epoch, rect_idx, html_node, window_size,
    } = rectangle;

    let rect = &display_rectangle_arena[*rect_idx];
    let bounds = layout_result.rects[*rect_idx].bounds;

    let display_list_rect_bounds = DisplayListRect::new(
         LogicalPosition::new(bounds.origin.x, bounds.origin.y),
         LogicalSize::new(bounds.size.width, bounds.size.height),
    );

    let tag_id = rect.tag.map(|tag| (tag, 0)).or({
        scrollable_nodes.overflowing_nodes
        .get(&rect_idx)
        .map(|scrolled| (scrolled.scroll_tag_id.0, 0))
    });

    let mut frame = DisplayListFrame {
        tag: tag_id,
        clip_rect: None,
        rect: display_list_rect_bounds,
        content: Vec::new(),
        children: Vec::new(),
    };

    let info = LayoutPrimitiveInfo {
        rect: bounds,
        clip_rect: bounds,
        is_backface_visible: false,
        tag: tag_id,
    };

    let border_radius = rect.style.border_radius.unwrap_or(StyleBorderRadius::zero());

    if let Some(box_shadow) = &rect.style.box_shadow {
        frame.content.push(DisplayListRectContent::BoxShadow {
            shadow: *box_shadow,
            border_radius: border_radius,
            clip_mode: BoxShadowClipMode::Outset,
        });
    }

    // If the rect is hit-testing relevant, we need to push a rect anyway.
    // Otherwise the hit-testing gets confused
    if let Some(bg) = &rect.style.background_attachement {

        use azul_css::StyleBackground::*;
        use azul_core::display_list::RectBackground;

        let style_bg = match bg {
            LinearGradient(lg) => RectBackground::LinearGradient(lg),
            RadialGradient(rg) => RectBackground::RadialGradient(rg),
            Image(style_image_id) => {
                let image_id = app_resources.get_css_image_id(&style_image_id.0)?;
                let image_info = app_resources.get_image_info(image_id)?;
                RectBackground::Image(image_info)
            },
            Color(c) => RectBackground::Color(c),
        };

        frame.content.push(DisplayListRectContent::Background {
            background: background_type,
            size: rect.style.background_size,
            offset: rect.style.background_position,
            repeat: rect.style.background_repeat.unwrap_or_default(),
        });
    }

    if let Some(border) = &rect.style.border {
        frame.content.push(DisplayListRectContent::Border {
            border: *border,
            radius: border_radius,
        });
    }

    match html_node {
        Div => { },
        Text(_) | Label(_) => {
            if let Some(layouted_glyphs) = layout_result.layouted_glyph_cache.get(&rect_idx).cloned() {
                let font_color = rect.style.font_color.unwrap_or(DEFAULT_FONT_COLOR).0;
                let positioned_words = &layout_result.positioned_word_cache[&rect_idx];
                let font_instance_key = positioned_words.1;

                frame.content.push(get_text(
                    display_list_rect_bounds,
                    window_size.dimensions,
                    layouted_glyphs,
                    font_instance_key,
                    font_color,
                    &rect.layout,
                ));
            }
        },
        Image(image_id) => {
            if let Some(image_info) = referenced_mutable_content.app_resources.get_image_info(image_id) {
                frame.content.push(DisplayListRectContent::Image {
                    size: LogicalSize::new(info.rect.size.width, info.rect.size.height),
                    offset: LogicalPosition::new(0.0, 0.0),
                    image_rendering: ImageRendering::Auto,
                    alpha_type: AlphaType::PremultipliedAlpha,
                    image_key: image_info.key,
                    background: ColorU::WHITE,
                });
            }
        },
        GlTexture(callback) => {
            frame.content.push(call_opengl_callback(callback, &info, rectangle, referenced_content, referenced_mutable_content));
        },
        IFrame(callback) => {
            frame.children.push(call_iframe_callback(callback, &info, scrollable_nodes, rectangle, referenced_content, referenced_mutable_content));
        },
    };

    if let Some(box_shadow) = &rect.style.box_shadow {
        frame.content.push(DisplayListRectContent::BoxShadow {
            shadow: *box_shadow,
            border_radius,
            clip_mode: BoxShadowClipMode::Inset,
        });
    }

    frame
}

#[inline]
fn call_opengl_callback<'a,'b,'c,'d,'e,'f, T>(
    (texture_callback, texture_stack_ptr): &(GlTextureCallback<T>, StackCheckedPointer<T>),
    info: &LayoutPrimitiveInfo,
    rectangle: &DisplayListRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'a,'b,'c,'d,'e, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T>,
) -> DisplayListRectContent {

    use compositor::{ActiveTexture, ACTIVE_GL_TEXTURES};
    use gleam::gl;
    use wr_translate::{hidpi_rect_from_bounds, wr_translate_image_key};
    use app_resources::FontImageApi;

    let bounds = hidpi_rect_from_bounds(
        info.rect,
        rectangle.window_size.hidpi_factor,
        rectangle.window_size.winit_hidpi_factor
    );

    let texture = {
        // Make sure that the app data is locked before invoking the callback
        let _lock = &mut referenced_mutable_content.app_data;
        let tex = (texture_callback.0)(&texture_stack_ptr, LayoutInfo {
            window: &mut *referenced_mutable_content.fake_window,
            resources: &referenced_mutable_content.app_resources,
        }, bounds);

        // Reset the framebuffer and SRGB color target to 0
        let gl_context = referenced_mutable_content.fake_window.get_gl_context();

        gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
        gl_context.disable(gl::FRAMEBUFFER_SRGB);
        gl_context.disable(gl::MULTISAMPLE);

        tex
    };

    let opaque = false;
    // The texture gets mapped 1:1 onto the display, so there is no need for mipmaps
    let allow_mipmaps = false;

    let texture_width = texture.width as f32;
    let texture_height = texture.height as f32;

    // Note: The ImageDescriptor has no effect on how large the image appears on-screen
    let descriptor = ImageDescriptor::new(texture.width as i32, texture.height as i32, ImageFormat::BGRA8, opaque, allow_mipmaps);
    let key = referenced_mutable_content.render_api.new_image_key();
    let external_image_id = ExternalImageId(new_opengl_texture_id() as u64);

    let data = ImageData::External(ExternalImageData {
        id: external_image_id,
        channel_index: 0,
        image_type: ExternalImageType::TextureHandle(TextureTarget::Default),
    });

    ACTIVE_GL_TEXTURES.lock().unwrap()
        .entry(rectangle.epoch).or_insert_with(|| FastHashMap::default())
        .insert(external_image_id, ActiveTexture { texture });

    referenced_mutable_content.resource_updates.push(ResourceUpdate::AddImage(
        AddImage { key: wr_translate_image_key(key), descriptor, data, tiling: None }
    ));

    DisplayListRectContent::Image {
        size: LogicalSize::new(texture_width as f32, texture_height as f32),
        offset: LogicalPosition::new(0.0, 0.0),
        image_rendering: ImageRendering::Auto,
        alpha_type: AlphaType::Alpha,
        image_key: key,
        background: ColorU::WHITE,
    }
}

#[inline]
fn call_iframe_callback<'a,'b,'c,'d,'e,'f, T>(
    (iframe_callback, iframe_pointer): &(IFrameCallback<T>, StackCheckedPointer<T>),
    info: &LayoutPrimitiveInfo,
    parent_scrollable_nodes: &mut ScrolledNodes,
    rectangle: &DisplayListRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'a,'b,'c,'d,'e, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T>,
) -> DisplayListMsg {

    use app_resources;
    use ui_state::ui_state_from_dom;
    use wr_translate::hidpi_rect_from_bounds;

    let bounds = hidpi_rect_from_bounds(
        info.rect,
        rectangle.window_size.hidpi_factor,
        rectangle.window_size.hidpi_factor
    );

    let new_dom = {
        // Make sure that the app data is locked before invoking the callback
        let _lock = &mut referenced_mutable_content.app_data;
        let window_info = LayoutInfo {
            window: referenced_mutable_content.fake_window,
            resources: &referenced_mutable_content.app_resources,
        };

        (iframe_callback.0)(&iframe_pointer, window_info, bounds)
    };

    // TODO: Right now, no focusing, hovering or :active allowed in iframes!
    let is_mouse_down = false;
    let mut focused_node = None;
    let mut focus_target = None;
    let hovered_nodes = BTreeMap::new();

    let mut ui_state = ui_state_from_dom(new_dom);
    let ui_description = UiDescription::<T>::match_css_to_dom(
        &mut ui_state,
        &referenced_content.css,
        &mut focused_node,
        &mut focus_target,
        &hovered_nodes,
        is_mouse_down
    );

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
    let rect_size = LayoutSize::new(
        info.rect.size.width / rectangle.window_size.hidpi_factor as f32 * rectangle.window_size.winit_hidpi_factor as f32,
        info.rect.size.height / rectangle.window_size.hidpi_factor as f32 * rectangle.window_size.winit_hidpi_factor as f32,
    );
    let rect_origin = LayoutPoint::new(info.rect.origin.x, info.rect.origin.y);
    let layout_result = do_the_layout(
        &node_hierarchy,
        &node_data,
        &display_list.rectangles,
        &*referenced_mutable_content.app_resources,
        rect_size,
        rect_origin,
    );

    let mut scrollable_nodes = get_nodes_that_need_scroll_clip(
        node_hierarchy, &display_list.rectangles, node_data, &layout_result.rects,
        &layout_result.node_depths, referenced_content.pipeline_id
    );

    let rects_in_rendering_order = determine_rendering_order(
        node_hierarchy, &display_list.rectangles, &layout_result.rects
    );

    let referenced_content = DisplayListParametersRef {
        // Important: Need to update the ui description,
        // otherwise this function would be endlessly recurse
        node_hierarchy,
        node_data,
        display_rectangle_arena: &display_list.rectangles,
        layout_result: &layout_result,
        .. *referenced_content
    };

    let display_list_msg = push_rectangles_into_displaylist(
        rectangle.epoch,
        rectangle.window_size,
        rects_in_rendering_order,
        &mut scrollable_nodes,
        &mut ScrollStates::new(),
        &referenced_content,
        referenced_mutable_content
    );

    parent_scrollable_nodes.overflowing_nodes.extend(scrollable_nodes.overflowing_nodes.into_iter());
    parent_scrollable_nodes.tags_to_node_ids.extend(scrollable_nodes.tags_to_node_ids.into_iter());

    display_list_msg
}

fn get_text(
    current_bounds: DisplayListRect,
    root_window_size: LogicalSize,
    layouted_glyphs: LayoutedGlyphs,
    font_instance_key: FontInstanceKey,
    font_color: ColorU,
    rect_layout: &RectLayout,
) -> DisplayListRectContent {

    let overflow_horizontal_visible = rect_layout.is_horizontal_overflow_visible();
    let overflow_vertical_visible = rect_layout.is_horizontal_overflow_visible();

    let original_text_bounds = rect_layout.padding
        .as_ref()
        .map(|padding| subtract_padding(&current_bounds, padding))
        .unwrap_or(current_bounds);

    // Adjust the bounds by the padding, depending on the overflow:visible parameter
    let text_clip_rect = match (overflow_horizontal_visible, overflow_vertical_visible) {
        (true, true) => None,
        (false, false) => Some(original_text_bounds),
        (true, false) => {
            // Horizontally visible, vertically cut
            Some(DisplayListRect {
                origin: current_bounds.origin,
                size: LogicalSize::new(root_window_size.width, original_text_bounds.size.height),
            })
        },
        (false, true) => {
            // Vertically visible, horizontally cut
            Some(DisplayListRect {
                origin: current_bounds.origin,
                size: LogicalSize::new(original_text_bounds.size.width, root_window_size.height),
            })
        },
    };

    DisplayListRectContent::Text {
        glyphs: layouted_glyphs.glyphs,
        font_instance_key,
        color: font_color,
        options: None,
        clip: text_clip_rect,
    }
}

/// Subtracts the padding from the bounds, returning the new bounds
///
/// Warning: The resulting rectangle may have negative width or height
fn subtract_padding(bounds: &DisplayListRect, padding: &LayoutPadding) -> DisplayListRect {

    let top     = padding.top.map(|top| top.to_pixels()).unwrap_or(0.0);
    let bottom  = padding.bottom.map(|bottom| bottom.to_pixels()).unwrap_or(0.0);
    let left    = padding.left.map(|left| left.to_pixels()).unwrap_or(0.0);
    let right   = padding.right.map(|right| right.to_pixels()).unwrap_or(0.0);

    let mut new_bounds = *bounds;

    new_bounds.origin.x += left;
    new_bounds.size.width -= right + left;
    new_bounds.origin.y += top;
    new_bounds.size.height -= top + bottom;

    new_bounds
}

/// Populate the style properties of the `DisplayRectangle`, apply static / dynamic properties
fn populate_css_properties(
    rect: &mut DisplayRectangle,
    node_id: NodeId,
    css_overrides: &BTreeMap<NodeId, FastHashMap<DomString, CssProperty>>
) {
    use azul_css::CssDeclaration::*;

    for constraint in rect.styled_node.css_constraints.values() {
        match &constraint {
            Static(static_property) => apply_style_property(rect, static_property),
            Dynamic(dynamic_property) => {
                let is_dynamic_prop = css_overrides.get(&node_id).and_then(|overrides| {
                    overrides.get(&DomString::Heap(dynamic_property.dynamic_id.clone()))
                });

                if let Some(overridden_property) = is_dynamic_prop {
                    // Only apply the dynamic style property default, if it isn't set to auto
                    if property_type_matches(overridden_property, &dynamic_property.default) {
                        apply_style_property(rect, overridden_property);
                    } else {
                        #[cfg(feature = "logging")] {
                            error!(
                                "Dynamic style property on rect {:?} don't have the same discriminant type,\r\n
                                cannot override {:?} with {:?} - enum discriminant mismatch",
                                rect, dynamic_property.default, overridden_property
                            )
                        }
                    }
                } else if let DynamicCssPropertyDefault::Exact(default) = &dynamic_property.default {
                    apply_style_property(rect, default);
                }
            }
        }
    }
}

// Assert that the types of two properties matches
fn property_type_matches(a: &CssProperty, b: &DynamicCssPropertyDefault) -> bool {
    use std::mem::discriminant;
    use azul_css::DynamicCssPropertyDefault::*;
    match b {
        Exact(e) => discriminant(a) == discriminant(e),
        Auto => true, // "auto" always matches
    }
}

fn apply_style_property(rect: &mut DisplayRectangle, property: &CssProperty) {

    use azul_css::CssProperty::*;

    match property {
        BorderRadius(b)     => { rect.style.border_radius = Some(*b);                   },
        BackgroundSize(s)   => { rect.style.background_size = Some(*s);                 },
        BackgroundRepeat(r) => { rect.style.background_repeat = Some(*r);               },
        TextColor(t)        => { rect.style.font_color = Some(*t);                      },
        Border(b)           => { StyleBorder::merge(&mut rect.style.border, &b);        },
        Background(b)       => { rect.style.background = Some(b.clone());               },
        FontSize(f)         => { rect.style.font_size = Some(*f);                       },
        FontFamily(f)       => { rect.style.font_family = Some(f.clone());              },
        LetterSpacing(l)    => { rect.style.letter_spacing = Some(*l);                  },
        TextAlign(ta)       => { rect.style.text_align = Some(*ta);                     },
        BoxShadow(b)        => { StyleBoxShadow::merge(&mut rect.style.box_shadow, b);  },
        LineHeight(lh)      => { rect.style.line_height = Some(*lh);                    },

        Width(w)            => { rect.layout.width = Some(*w);                          },
        Height(h)           => { rect.layout.height = Some(*h);                         },
        MinWidth(mw)        => { rect.layout.min_width = Some(*mw);                     },
        MinHeight(mh)       => { rect.layout.min_height = Some(*mh);                    },
        MaxWidth(mw)        => { rect.layout.max_width = Some(*mw);                     },
        MaxHeight(mh)       => { rect.layout.max_height = Some(*mh);                    },

        Position(p)         => { rect.layout.position = Some(*p);                       },
        Top(t)              => { rect.layout.top = Some(*t);                            },
        Bottom(b)           => { rect.layout.bottom = Some(*b);                         },
        Right(r)            => { rect.layout.right = Some(*r);                          },
        Left(l)             => { rect.layout.left = Some(*l);                           },

        Padding(p)          => { LayoutPadding::merge(&mut rect.layout.padding, &p);    },
        Margin(m)           => { LayoutMargin::merge(&mut rect.layout.margin, &m);      },
        Overflow(o)         => { LayoutOverflow::merge(&mut rect.layout.overflow, &o);  },
        WordSpacing(ws)     => { rect.style.word_spacing = Some(*ws);                   },
        TabWidth(tw)        => { rect.style.tab_width = Some(*tw);                      },

        FlexGrow(g)         => { rect.layout.flex_grow = Some(*g)                       },
        FlexShrink(s)       => { rect.layout.flex_shrink = Some(*s)                     },
        FlexWrap(w)         => { rect.layout.wrap = Some(*w);                           },
        FlexDirection(d)    => { rect.layout.direction = Some(*d);                      },
        JustifyContent(j)   => { rect.layout.justify_content = Some(*j);                },
        AlignItems(a)       => { rect.layout.align_items = Some(*a);                    },
        AlignContent(a)     => { rect.layout.align_content = Some(*a);                  },
        Cursor(_)           => { /* cursor neither affects layout nor styling */        },
    }
}

#[test]
fn test_overflow_parsing() {

    use azul_css::Overflow;

    let layout1 = RectLayout::default();

    // The default for overflowing is overflow: auto, which clips
    // children, so this should evaluate to true by default
    assert_eq!(node_needs_to_clip_children(&layout1), true);

    let layout2 = RectLayout {
        overflow: Some(LayoutOverflow {
            horizontal: Some(Overflow::Visible),
            vertical: Some(Overflow::Visible),
        }),
        .. Default::default()
    };
    assert_eq!(node_needs_to_clip_children(&layout2), false);

    let layout3 = RectLayout {
        overflow: Some(LayoutOverflow {
            horizontal: Some(Overflow::Hidden),
            vertical: Some(Overflow::Visible),
        }),
        .. Default::default()
    };
    assert_eq!(node_needs_to_clip_children(&layout3), true);
}