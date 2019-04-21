#![allow(unused_variables)]

use std::{
    fmt,
    sync::{Arc, Mutex},
    collections::BTreeMap,
};
use euclid::{TypedRect, TypedSize2D};
use webrender::api::{
    LayoutPixel, DisplayListBuilder, PrimitiveInfo, GradientStop,
    ColorF, Epoch, ImageData, ImageDescriptor,
    ResourceUpdate, AddImage, BorderRadius, ClipMode,
    LayoutPoint, LayoutSize, GlyphOptions, LayoutRect, ExternalScrollId,
    ComplexClipRegion, LayoutPrimitiveInfo, ExternalImageId,
    ExternalImageData, ImageFormat, ExternalImageType, TextureTarget,
    ImageRendering, AlphaType, FontInstanceFlags, FontRenderMode,
    RenderApi,
};
use azul_css::{
    Css, LayoutPosition,CssProperty, LayoutOverflow,
    StyleBorderRadius, LayoutMargin, LayoutPadding, BoxShadowClipMode,
    StyleTextColor, StyleBackground, StyleBoxShadow,
    StyleBackgroundSize, StyleBackgroundRepeat, StyleBorder, BoxShadowPreDisplayItem,
    RectStyle, RectLayout, ColorU as StyleColorU, DynamicCssPropertyDefault,
};
use {
    FastHashMap,
    app_resources::AppResources,
    callbacks::{IFrameCallback, GlTextureCallback, HidpiAdjustedBounds, StackCheckedPointer},
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
};
use azul_core::callbacks::PipelineId;

const DEFAULT_FONT_COLOR: StyleTextColor = StyleTextColor(StyleColorU { r: 0, b: 0, g: 0, a: 255 });

pub(crate) struct DisplayList<'a, T: 'a> {
    pub(crate) ui_descr: &'a UiDescription<T>,
    pub(crate) rectangles: NodeDataContainer<DisplayRectangle<'a>>
}

impl<'a, T: 'a> fmt::Debug for DisplayList<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "DisplayList {{ ui_descr: {:?}, rectangles: {:?} }}",
            self.ui_descr, self.rectangles
        )
    }
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

impl<'a, T: 'a> DisplayList<'a, T> {

    /// NOTE: This function assumes that the UiDescription has an initialized arena
    ///
    /// This only looks at the user-facing styles of the `UiDescription`, not the actual
    /// layout. The layout is done only in the `into_display_list_builder` step.
    pub(crate) fn new_from_ui_description(ui_description: &'a UiDescription<T>, ui_state: &UiState<T>) -> Self {
        let arena = &ui_description.ui_descr_arena;

        let display_rect_arena = arena.node_data.transform(|node, node_id| {
            let style = &ui_description.styled_nodes[node_id];
            let tag = ui_state.node_ids_to_tag_ids.get(&node_id).map(|tag| *tag);
            let mut rect = DisplayRectangle::new(tag, style);
            populate_css_properties(&mut rect, node_id, &ui_description.dynamic_css_overrides);
            rect
        });

        Self {
            ui_descr: ui_description,
            rectangles: display_rect_arena,
        }
    }

    /// Inserts and solves the top-level DOM (i.e. the DOM with the ID 0)
    pub(crate) fn into_display_list_builder(
        &self,
        app_data_access: &mut Arc<Mutex<T>>,
        window: &mut Window<T>,
        fake_window: &mut FakeWindow<T>,
        app_resources: &mut AppResources,
        render_api: &mut RenderApi,
    ) -> (DisplayListBuilder, ScrolledNodes, LayoutResult) {

        use window::LogicalSize;
        use app_resources::add_fonts_and_images;
        use wr_translate::translate_pipeline_id;

        let mut resource_updates = Vec::<ResourceUpdate>::new();

        let arena = &self.ui_descr.ui_descr_arena;
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
        add_fonts_and_images(app_resources, render_api, &self);

        let window_size = window.state.size.get_reverse_logical_size();
        let layout_result = do_the_layout(
            node_hierarchy,
            node_data,
            &self.rectangles,
            &*app_resources,
            LayoutSize::new(window_size.width as f32, window_size.height as f32),
            LayoutPoint::new(0.0, 0.0),
        );

        // TODO: After the layout has been done, call all IFrameCallbacks and get and insert
        // their font keys / image keys

        let mut scrollable_nodes = get_nodes_that_need_scroll_clip(
            node_hierarchy, &self.rectangles, node_data, &layout_result.rects,
            &layout_result.node_depths, window.internal.pipeline_id
        );

        // Make sure unused scroll states are garbage collected.
        window.scroll_states.remove_unused_scroll_states();

        let LogicalSize { width, height } = window.state.size.dimensions;
        let mut builder = DisplayListBuilder::with_capacity(
            translate_pipeline_id(window.internal.pipeline_id),
            TypedSize2D::new(width as f32, height as f32),
            self.rectangles.len()
        );

        let rects_in_rendering_order = determine_rendering_order(
            node_hierarchy,
            &self.rectangles,
            &layout_result.rects
        );

        push_rectangles_into_displaylist(
            window.internal.epoch,
            window.state.size,
            rects_in_rendering_order,
            &mut scrollable_nodes,
            &mut window.scroll_states,
            &DisplayListParametersRef {
                pipeline_id: window.internal.pipeline_id,
                node_hierarchy,
                node_data,
                display_rectangle_arena: &self.rectangles,
                css: &window.css,
                layout_result: &layout_result,
            },
            &mut DisplayListParametersMut {
                app_data: app_data_access,
                app_resources,
                fake_window,
                builder: &mut builder,
                resource_updates: &mut resource_updates,
                render_api,
            },
        );

        (builder, scrollable_nodes, layout_result)
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
)
{
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

#[derive(Default, Debug, Clone)]
pub(crate)  struct ScrolledNodes {
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
        let parent_external_scroll_id  = ExternalScrollId(parent_dom_hash.0, wr_translate::translate_pipeline_id(pipeline_id));

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

fn push_rectangles_into_displaylist<'a, 'b, 'c, 'd, 'e, 'f, T>(
    epoch: Epoch,
    window_size: WindowSize,
    content_grouped_rectangles: ContentGroupOrder,
    scrollable_nodes: &mut ScrolledNodes,
    scroll_states: &mut ScrollStates,
    referenced_content: &DisplayListParametersRef<'a,'b,'c,'d,'e, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T>)
{
    let mut clip_stack = Vec::new();

    for content_group in content_grouped_rectangles.groups {
        let rectangle = DisplayListRectParams {
            epoch,
            rect_idx: content_group.root.node_id,
            html_node: referenced_content.node_data[content_group.root.node_id].get_node_type(),
            window_size,
        };

        // Push the root of the node
        push_rectangles_into_displaylist_inner(
            content_group.root,
            scrollable_nodes,
            &rectangle,
            referenced_content,
            referenced_mutable_content,
            &mut clip_stack
        );

        for item in content_group.node_ids {

            let rectangle = DisplayListRectParams {
                epoch,
                rect_idx: item.node_id,
                html_node: referenced_content.node_data[item.node_id].get_node_type(),
                window_size,
            };

            push_rectangles_into_displaylist_inner(
                item,
                scrollable_nodes,
                &rectangle,
                referenced_content,
                referenced_mutable_content,
                &mut clip_stack
            );
        }
    }
}

fn push_rectangles_into_displaylist_inner<'a,'b,'c,'d,'e,'f, T>(
    item: RenderableNodeId,
    scrollable_nodes: &mut ScrolledNodes,
    rectangle: &DisplayListRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'a,'b,'c,'d,'e, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T>,
    clip_stack: &mut Vec<NodeId>,
) {
    displaylist_handle_rect(
        scrollable_nodes,
        rectangle,
        referenced_content,
        referenced_mutable_content
    );
/*

    // NOTE: table demo has problems with clipping

    if item.clip_children {
        if let Some(last_child) = referenced_content.node_hierarchy[rectangle.rect_idx].last_child {
            let styled_node = &referenced_content.display_rectangle_arena[rectangle.rect_idx];
            let solved_rect = &referenced_content.layout_result.rects[rectangle.rect_idx];
            let clip = get_clip_region(solved_rect.bounds, &styled_node)
                .unwrap_or(ComplexClipRegion::new(solved_rect.bounds, BorderRadius::zero(), ClipMode::Clip));
            let clip_id = referenced_mutable_content.builder.define_clip(solved_rect.bounds, vec![clip], /* image_mask: */ None);
            referenced_mutable_content.builder.push_clip_id(clip_id);
            clip_stack.push(last_child);
        }
    }

    if clip_stack.last().cloned() == Some(rectangle.rect_idx) {
        referenced_mutable_content.builder.pop_clip_id();
        clip_stack.pop();
    }
*/
}

/// Parameters that apply to a single rectangle / div node
#[derive(Copy, Clone)]
pub(crate) struct DisplayListRectParams<'a, T: 'a> {
    pub epoch: Epoch,
    pub rect_idx: NodeId,
    pub html_node: &'a NodeType<T>,
    window_size: WindowSize,
}

fn get_clip_region<'a>(bounds: LayoutRect, rect: &DisplayRectangle<'a>) -> Option<ComplexClipRegion> {
    use css::webrender_translate::wr_translate_border_radius;
    rect.style.border_radius.and_then(|border_radius| {
        Some(ComplexClipRegion {
            rect: bounds,
            radii: wr_translate_border_radius(border_radius.0).into(),
            mode: ClipMode::Clip,
        })
    })
}

/// Push a single rectangle into the display list builder
#[inline]
fn displaylist_handle_rect<'a,'b,'c,'d,'e,'f,'g, T>(
    scrollable_nodes: &mut ScrolledNodes,
    rectangle: &DisplayListRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'b,'c,'d,'e,'f, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'g, T>)
{
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

    let info = LayoutPrimitiveInfo {
        rect: bounds,
        clip_rect: bounds,
        is_backface_visible: false,
        tag: rect.tag.map(|tag| (tag, 0)).or({
            scrollable_nodes.overflowing_nodes
            .get(&rect_idx)
            .map(|scrolled| (scrolled.scroll_tag_id.0, 0))
        }),
    };

    let clip_region_id = get_clip_region(bounds, &rect).map(|clip|
        referenced_mutable_content.builder.define_clip(bounds, vec![clip], None)
    );

    // Push the "outset" box shadow, before the clip is active
    push_box_shadow(
        referenced_mutable_content.builder,
        &rect.style,
        &bounds,
        BoxShadowClipMode::Outset,
    );

    if let Some(id) = clip_region_id {
        referenced_mutable_content.builder.push_clip_id(id);
    }

    // If the rect is hit-testing relevant, we need to push a rect anyway.
    // Otherwise the hit-testing gets confused
    if let Some(bg) = &rect.style.background {
        push_background(
            &info,
            &bounds,
            referenced_mutable_content.builder,
            bg,
            &rect.style.background_size,
            &rect.style.background_repeat,
            &referenced_mutable_content.app_resources,
        );
    } else if info.tag.is_some() {
        const TRANSPARENT_BG: StyleColorU = StyleColorU { r: 0, g: 0, b: 0, a: 0 };
        push_rect(
            &info,
            referenced_mutable_content.builder,
            &TRANSPARENT_BG,
        );
    }

    if let Some(ref border) = rect.style.border {
        push_border(
            &info,
            referenced_mutable_content.builder,
            &border,
            &rect.style.border_radius,
        );
    }

    match html_node {
        Div => { },
        Text(_) | Label(_) => {
            // Text is laid out and positioned during the layout pass,
            // so this should succeed - if there were problems
            //
            // TODO: In the table demo, the numbers don't show - empty glyphs (why?)!
            push_text(
                &info,
                referenced_mutable_content.builder,
                layout_result,
                rect_idx,
                &rect.style,
                &rect.layout,
            )
        },
        Image(image_id) => push_image(
            &info,
            referenced_mutable_content.builder,
            referenced_mutable_content.app_resources,
            image_id,
            LayoutSize::new(info.rect.size.width, info.rect.size.height)
        ),
        GlTexture(callback) => push_opengl_texture(callback, &info, rectangle, referenced_content, referenced_mutable_content),
        IFrame(callback) => push_iframe(callback, &info, scrollable_nodes, rectangle, referenced_content, referenced_mutable_content),
    };

    // Push the inset shadow (if any)
    push_box_shadow(
        referenced_mutable_content.builder,
        &rect.style,
        &bounds,
        BoxShadowClipMode::Inset
    );

    if clip_region_id.is_some() {
        referenced_mutable_content.builder.pop_clip_id();
    }
}

fn push_opengl_texture<'a,'b,'c,'d,'e,'f, T>(
    (texture_callback, texture_stack_ptr): &(GlTextureCallback<T>, StackCheckedPointer<T>),
    info: &LayoutPrimitiveInfo,
    rectangle: &DisplayListRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'a,'b,'c,'d,'e, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T>,
) {
    use compositor::{ActiveTexture, ACTIVE_GL_TEXTURES};
    use gleam::gl;
    use app_resources::FontImageApi;

    let bounds = HidpiAdjustedBounds::from_bounds(
        info.rect,
        rectangle.window_size.hidpi_factor,
        rectangle.window_size.winit_hidpi_factor
    );

    let texture;

    {
        // Make sure that the app data is locked before invoking the callback
        let _lock = referenced_mutable_content.app_data.lock().unwrap();
        texture = (texture_callback.0)(&texture_stack_ptr, LayoutInfo {
            window: &mut *referenced_mutable_content.fake_window,
            resources: &referenced_mutable_content.app_resources,
        }, bounds);

        // Reset the framebuffer and SRGB color target to 0
        let gl_context = referenced_mutable_content.fake_window.get_gl_context();

        gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
        gl_context.disable(gl::FRAMEBUFFER_SRGB);
        gl_context.disable(gl::MULTISAMPLE);
    }

    let opaque = false;
    // The texture gets mapped 1:1 onto the display, so there is no need for mipmaps
    let allow_mipmaps = false;

    let texture_width = texture.width as f32;
    let texture_height = texture.height as f32;

    // Note: The ImageDescriptor has no effect on how large the image appears on-screen
    let descriptor = ImageDescriptor::new(texture.width as i32, texture.height as i32, ImageFormat::BGRA8, opaque, allow_mipmaps);
    let key = referenced_mutable_content.app_resources.get_render_api().new_image_key();
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
        AddImage { key, descriptor, data, tiling: None }
    ));

    referenced_mutable_content.builder.push_image(
        &info,
        LayoutSize::new(texture_width as f32, texture_height as f32),
        LayoutSize::zero(),
        ImageRendering::Auto,
        AlphaType::Alpha,
        key,
        ColorF::WHITE
    );
}

fn push_iframe<'a,'b,'c,'d,'e,'f, T>(
    (iframe_callback, iframe_pointer): &(IFrameCallback<T>, StackCheckedPointer<T>),
    info: &LayoutPrimitiveInfo,
    parent_scrollable_nodes: &mut ScrolledNodes,
    rectangle: &DisplayListRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'a,'b,'c,'d,'e, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T>,
) {
    use app_resources;

    let bounds = HidpiAdjustedBounds::from_bounds(
        info.rect,
        rectangle.window_size.hidpi_factor,
        rectangle.window_size.hidpi_factor
    );

    let new_dom = {
        // Make sure that the app data is locked before invoking the callback
        let _lock = referenced_mutable_content.app_data.lock().unwrap();

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

    let mut ui_state = new_dom.into_ui_state();
    let ui_description = UiDescription::<T>::match_css_to_dom(
        &mut ui_state,
        &referenced_content.css,
        &mut focused_node,
        &mut focus_target,
        &hovered_nodes,
        is_mouse_down
    );

    let display_list = DisplayList::new_from_ui_description(&ui_description, &ui_state);
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
        // Important: Need to update the ui description, otherwise this function would be endlessly recursive
        node_hierarchy,
        node_data,
        display_rectangle_arena: &display_list.rectangles,
        layout_result: &layout_result,
        .. *referenced_content
    };

    push_rectangles_into_displaylist(
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
    pub app_data: &'a mut Arc<Mutex<T>>,
    /// The original, top-level display list builder that we need to push stuff into
    pub builder: &'a mut DisplayListBuilder,
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

fn push_rect(
    info: &PrimitiveInfo<LayoutPixel>,
    builder: &mut DisplayListBuilder,
    color: &StyleColorU
) {
    use css::webrender_translate::wr_translate_color_u;
    builder.push_rect(&info, wr_translate_color_u(*color).into());
}

fn push_text(
    info: &PrimitiveInfo<LayoutPixel>,
    builder: &mut DisplayListBuilder,
    layout_result: &LayoutResult,
    node_id: &NodeId,
    rect_style: &RectStyle,
    rect_layout: &RectLayout,
) {
    use text_layout::get_layouted_glyphs;
    use css::webrender_translate::wr_translate_color_u;
    use ui_solver::determine_text_alignment;

    let (scaled_words, _font_instance_key) = match layout_result.scaled_words.get(node_id) {
        Some(s) => s,
        None => return,
    };

    let (word_positions, font_instance_key) = match layout_result.positioned_word_cache.get(node_id) {
        Some(s) => s,
        None => return,
    };

    let (horz_alignment, vert_alignment) = determine_text_alignment(rect_style, rect_layout);

    let rect_padding_top = rect_layout.padding.unwrap_or_default().top.map(|top| top.to_pixels()).unwrap_or(0.0);
    let rect_padding_left = rect_layout.padding.unwrap_or_default().left.map(|left| left.to_pixels()).unwrap_or(0.0);
    let rect_offset = LayoutPoint::new(info.rect.origin.x + rect_padding_left, info.rect.origin.y + rect_padding_top);
    let bounding_size_height_px = info.rect.size.height - rect_layout.get_vertical_padding();

    let layouted_glyphs = get_layouted_glyphs(
        word_positions,
        scaled_words,
        horz_alignment,
        vert_alignment,
        rect_offset.clone(),
        bounding_size_height_px
    );

    let font_color = rect_style.font_color.unwrap_or(DEFAULT_FONT_COLOR).0;
    let font_color = wr_translate_color_u(font_color);

    // WARNING: Do not enable FontInstanceFlags::FONT_SMOOTHING or FontInstanceFlags::FORCE_AUTOHINT -
    // they seem to interfere with the text layout thereby messing with the actual text layout.
    let mut flags = FontInstanceFlags::empty();
    flags.set(FontInstanceFlags::SUBPIXEL_BGR, true);
    flags.set(FontInstanceFlags::NO_AUTOHINT, true);
    flags.set(FontInstanceFlags::LCD_VERTICAL, true);

    let overflow_horizontal_visible = rect_layout.is_horizontal_overflow_visible();
    let overflow_vertical_visible = rect_layout.is_horizontal_overflow_visible();

    let max_bounds = builder.content_size();
    let current_bounds = info.rect;
    let original_text_bounds = rect_layout.padding
        .as_ref()
        .map(|padding| subtract_padding(&current_bounds, padding))
        .unwrap_or(current_bounds);

    // Adjust the bounds by the padding, depending on the overflow:visible parameter
    let mut text_bounds = match (overflow_horizontal_visible, overflow_vertical_visible) {
        (true, true) => None,
        (false, false) => Some(original_text_bounds),
        (true, false) => {
            // Horizontally visible, vertically cut
            Some(LayoutRect::new(rect_offset, LayoutSize::new(max_bounds.width, original_text_bounds.size.height)))
        },
        (false, true) => {
            // Vertically visible, horizontally cut
            Some(LayoutRect::new(rect_offset, LayoutSize::new(original_text_bounds.size.width, max_bounds.height)))
        },
    };

    if let Some(text_bounds) = &mut text_bounds {
        text_bounds.size.width = text_bounds.size.width.max(0.0);
        text_bounds.size.height = text_bounds.size.height.max(0.0);
        let clip_id = builder.define_clip(*text_bounds, vec![ComplexClipRegion {
            rect: *text_bounds,
            radii: BorderRadius::zero(),
            mode: ClipMode::Clip,
        }], None);
        builder.push_clip_id(clip_id);
    }

    builder.push_text(
        &info,
        &layouted_glyphs.glyphs,
        *font_instance_key,
        font_color.into(),
        Some(GlyphOptions {
            render_mode: FontRenderMode::Subpixel,
            flags: flags,
        })
    );

    if text_bounds.is_some() {
        builder.pop_clip_id();
    }
}

enum ShouldPushShadow {
    OneShadow,
    TwoShadows,
    AllShadows,
}

/// WARNING: For "inset" shadows, you must push a clip ID first, otherwise the
/// shadow will not show up.
///
/// To prevent a shadow from being pushed twice, you have to annotate the clip
/// mode for this - outset or inset.
#[inline]
fn push_box_shadow(
    builder: &mut DisplayListBuilder,
    style: &RectStyle,
    bounds: &LayoutRect,
    shadow_type: BoxShadowClipMode)
{
    use self::ShouldPushShadow::*;

    // Box-shadow can be applied to each corner separately. This means, in practice
    // that we simply overlay multiple shadows with shifted clipping rectangles
    let StyleBoxShadow { top, left, bottom, right } = match &style.box_shadow {
        Some(s) => s,
        None => return,
    };

    let border_radius = style.border_radius.unwrap_or(StyleBorderRadius::zero());

    let what_shadow_to_push = match [top, left, bottom, right].iter().filter(|x| x.is_some()).count() {
        1 => OneShadow,
        2 => TwoShadows,
        4 => AllShadows,
        _ => return,
    };

    match what_shadow_to_push {
        OneShadow => {
            let current_shadow = match (top, left, bottom, right) {
                 | (Some(Some(shadow)), None, None, None)
                 | (None, Some(Some(shadow)), None, None)
                 | (None, None, Some(Some(shadow)), None)
                 | (None, None, None, Some(Some(shadow)))
                 => shadow,
                 _ => return, // reachable, but invalid box-shadow
            };

            push_single_box_shadow_edge(
                builder, current_shadow, bounds, border_radius, shadow_type,
                top, bottom, left, right
            );
        },
        // Two shadows in opposite directions:
        //
        // box-shadow-top: 0px 0px 5px red;
        // box-shadow-bottom: 0px 0px 5px blue;
        TwoShadows => {
            match (top, left, bottom, right) {

                // top + bottom box-shadow pair
                (Some(Some(t)), None, Some(Some(b)), right) => {
                    push_single_box_shadow_edge(
                        builder, t, bounds, border_radius, shadow_type,
                        top, &None, &None, &None
                    );
                    push_single_box_shadow_edge(
                        builder, b, bounds, border_radius, shadow_type,
                        &None, bottom, &None, &None
                    );
                },
                // left + right box-shadow pair
                (None, Some(Some(l)), None, Some(Some(r))) => {
                    push_single_box_shadow_edge(
                        builder, l, bounds, border_radius, shadow_type,
                        &None, &None, left, &None
                    );
                    push_single_box_shadow_edge(
                        builder, r, bounds, border_radius, shadow_type,
                        &None, &None, &None, right
                    );
                }
                _ => return, // reachable, but invalid
            }
        },
        AllShadows => {

            // Assumes that all box shadows are the same, so just use the top shadow
            let top_shadow = top.unwrap();
            let clip_rect = top_shadow
                .as_ref()
                .map(|top_shadow| get_clip_rect(top_shadow, bounds))
                .unwrap_or(*bounds);

            push_box_shadow_inner(
                builder,
                &top_shadow,
                border_radius,
                bounds,
                clip_rect,
                shadow_type
            );
        }
    }
}

fn push_box_shadow_inner(
    builder: &mut DisplayListBuilder,
    pre_shadow: &Option<BoxShadowPreDisplayItem>,
    border_radius: StyleBorderRadius,
    bounds: &LayoutRect,
    clip_rect: LayoutRect,
    shadow_type: BoxShadowClipMode)
{
    use webrender::api::LayoutVector2D;
    use css::webrender_translate::{
        wr_translate_color_u, wr_translate_border_radius,
        wr_translate_box_shadow_clip_mode
    };

    let pre_shadow = match pre_shadow {
        None => return,
        Some(ref s) => s,
    };

    // The pre_shadow is missing the StyleBorderRadius & LayoutRect
    if pre_shadow.clip_mode != shadow_type {
        return;
    }

    let full_screen_rect = LayoutRect::new(LayoutPoint::zero(), builder.content_size());;

    // prevent shadows that are larger than the full screen
    let clip_rect = clip_rect.intersection(&full_screen_rect).unwrap_or(clip_rect);

    // Apply a gamma of 2.2 to the original value
    //
    // NOTE: strangely box-shadow is the only thing that needs to be gamma-corrected...
    fn apply_gamma(color: ColorF) -> ColorF {

        const GAMMA: f32 = 2.2;
        const GAMMA_F: f32 = 1.0 / GAMMA;

        ColorF {
            r: color.r.powf(GAMMA_F),
            g: color.g.powf(GAMMA_F),
            b: color.b.powf(GAMMA_F),
            a: color.a,
        }
    }

    let info = LayoutPrimitiveInfo::with_clip_rect(LayoutRect::zero(), clip_rect);
    builder.push_box_shadow(
        &info,
        *bounds,
        LayoutVector2D::new(pre_shadow.offset[0].to_pixels(), pre_shadow.offset[1].to_pixels()),
        apply_gamma(wr_translate_color_u(pre_shadow.color).into()),
        pre_shadow.blur_radius.to_pixels(),
        pre_shadow.spread_radius.to_pixels(),
        wr_translate_border_radius(border_radius.0).into(),
        wr_translate_box_shadow_clip_mode(pre_shadow.clip_mode)
    );
}

fn get_clip_rect(pre_shadow: &BoxShadowPreDisplayItem, bounds: &LayoutRect) -> LayoutRect {
    if pre_shadow.clip_mode == BoxShadowClipMode::Inset {
        // inset shadows do not work like outset shadows
        // for inset shadows, you have to push a clip ID first, so that they are
        // clipped to the bounds -we trust that the calling function knows to do this
        *bounds
    } else {
        // calculate the maximum extent of the outset shadow
        let mut clip_rect = *bounds;

        let origin_displace = (pre_shadow.spread_radius.to_pixels() + pre_shadow.blur_radius.to_pixels()) * 2.0;
        clip_rect.origin.x = clip_rect.origin.x - pre_shadow.offset[0].to_pixels() - origin_displace;
        clip_rect.origin.y = clip_rect.origin.y - pre_shadow.offset[1].to_pixels() - origin_displace;

        clip_rect.size.height = clip_rect.size.height + (origin_displace * 2.0);
        clip_rect.size.width = clip_rect.size.width + (origin_displace * 2.0);
        clip_rect
    }
}

#[allow(clippy::collapsible_if)]
fn push_single_box_shadow_edge(
        builder: &mut DisplayListBuilder,
        current_shadow: &BoxShadowPreDisplayItem,
        bounds: &LayoutRect,
        border_radius: StyleBorderRadius,
        shadow_type: BoxShadowClipMode,
        top: &Option<Option<BoxShadowPreDisplayItem>>,
        bottom: &Option<Option<BoxShadowPreDisplayItem>>,
        left: &Option<Option<BoxShadowPreDisplayItem>>,
        right: &Option<Option<BoxShadowPreDisplayItem>>,
) {
    let is_inset_shadow = current_shadow.clip_mode == BoxShadowClipMode::Inset;
    let origin_displace = (current_shadow.spread_radius.to_pixels() + current_shadow.blur_radius.to_pixels()) * 2.0;

    let mut shadow_bounds = *bounds;
    let mut clip_rect = *bounds;

    if is_inset_shadow {
        // If the shadow is inset, we adjust the clip rect to be
        // exactly the amount of the shadow
        if let Some(Some(top)) = top {
            clip_rect.size.height = origin_displace;
            shadow_bounds.size.width += origin_displace;
            shadow_bounds.origin.x -= origin_displace / 2.0;
        } else if let Some(Some(bottom)) = bottom {
            clip_rect.size.height = origin_displace;
            clip_rect.origin.y += bounds.size.height - origin_displace;
            shadow_bounds.size.width += origin_displace;
            shadow_bounds.origin.x -= origin_displace / 2.0;
        } else if let Some(Some(left)) = left {
            clip_rect.size.width = origin_displace;
            shadow_bounds.size.height += origin_displace;
            shadow_bounds.origin.y -= origin_displace / 2.0;
        } else if let Some(Some(right)) = right {
            clip_rect.size.width = origin_displace;
            clip_rect.origin.x += bounds.size.width - origin_displace;
            shadow_bounds.size.height += origin_displace;
            shadow_bounds.origin.y -= origin_displace / 2.0;
        }
    } else {
        if let Some(Some(top)) = top {
            clip_rect.size.height = origin_displace;
            clip_rect.origin.y -= origin_displace;
            shadow_bounds.size.width += origin_displace;
            shadow_bounds.origin.x -= origin_displace / 2.0;
        } else if let Some(Some(bottom)) = bottom {
            clip_rect.size.height = origin_displace;
            clip_rect.origin.y += bounds.size.height;
            shadow_bounds.size.width += origin_displace;
            shadow_bounds.origin.x -= origin_displace / 2.0;
        } else if let Some(Some(left)) = left {
            clip_rect.size.width = origin_displace;
            clip_rect.origin.x -= origin_displace;
            shadow_bounds.size.height += origin_displace;
            shadow_bounds.origin.y -= origin_displace / 2.0;
        } else if let Some(Some(right)) = right {
            clip_rect.size.width = origin_displace;
            clip_rect.origin.x += bounds.size.width;
            shadow_bounds.size.height += origin_displace;
            shadow_bounds.origin.y -= origin_displace / 2.0;
        }
    }

    push_box_shadow_inner(
        builder,
        &Some(*current_shadow),
        border_radius,
        &shadow_bounds,
        clip_rect,
        shadow_type
    );
}

#[inline]
fn push_background(
    info: &PrimitiveInfo<LayoutPixel>,
    bounds: &TypedRect<f32, LayoutPixel>,
    builder: &mut DisplayListBuilder,
    background: &StyleBackground,
    background_size: &Option<StyleBackgroundSize>,
    background_repeat: &Option<StyleBackgroundRepeat>,
    app_resources: &AppResources)
{
    use azul_css::{Shape, StyleBackground::*};
    use css::webrender_translate::{
        wr_translate_color_u, wr_translate_extend_mode, wr_translate_layout_point,
        wr_translate_layout_rect,
    };

    match background {
        RadialGradient(gradient) => {
            let stops: Vec<GradientStop> = gradient.stops.iter().map(|gradient_pre|
                GradientStop {
                    offset: gradient_pre.offset.unwrap().get(),
                    color: wr_translate_color_u(gradient_pre.color).into(),
                }).collect();

            let center = bounds.center();

            // Note: division by 2.0 because it's the radius, not the diameter
            let radius = match gradient.shape {
                Shape::Ellipse => TypedSize2D::new(bounds.size.width / 2.0, bounds.size.height / 2.0),
                Shape::Circle => {
                    let largest_bound_size = bounds.size.width.max(bounds.size.height);
                    TypedSize2D::new(largest_bound_size / 2.0, largest_bound_size / 2.0)
                },
            };

            let gradient = builder.create_radial_gradient(center, radius, stops, wr_translate_extend_mode(gradient.extend_mode));
            builder.push_radial_gradient(&info, gradient, bounds.size, LayoutSize::zero());
        },
        LinearGradient(gradient) => {

            let stops: Vec<GradientStop> = gradient.stops.iter().map(|gradient_pre|
                GradientStop {
                    offset: gradient_pre.offset.unwrap().get() / 100.0,
                    color: wr_translate_color_u(gradient_pre.color).into(),
                }).collect();

            let (begin_pt, end_pt) = gradient.direction.to_points(&wr_translate_layout_rect(*bounds));
            let gradient = builder.create_gradient(
                wr_translate_layout_point(begin_pt),
                wr_translate_layout_point(end_pt),
                stops,
                wr_translate_extend_mode(gradient.extend_mode),
            );

            builder.push_gradient(&info, gradient, bounds.size, LayoutSize::zero());
        },
        Image(style_image_id) => {
            // TODO: background-origin, background-position, background-repeat
            if let Some(image_id) = app_resources.get_css_image_id(&style_image_id.0) {

                let bounds = info.rect;
                let image_dimensions = app_resources.get_image_info(image_id)
                    .map(|info| (info.descriptor.dimensions.width, info.descriptor.dimensions.height))
                    .unwrap_or((bounds.size.width as i32, bounds.size.height as i32)); // better than crashing...

                let size = match background_size {
                    Some(bg_size) => calculate_background_size(bg_size, &info, &image_dimensions),
                    None => TypedSize2D::new(image_dimensions.0 as f32, image_dimensions.1 as f32),
                };

                let background_repeat = background_repeat.unwrap_or_default();
                let background_repeat_info = get_background_repeat_info(&info, background_repeat, size);

                push_image(&background_repeat_info, builder, app_resources, image_id, size);
            }
        },
        Color(c) => {
            push_rect(&info, builder, c);
        },
        NoBackground => { },
    }
}

fn get_background_repeat_info(
    info: &LayoutPrimitiveInfo,
    background_repeat: StyleBackgroundRepeat,
    background_size: TypedSize2D<f32, LayoutPixel>,
) -> LayoutPrimitiveInfo {
    use azul_css::StyleBackgroundRepeat::*;
    match background_repeat {
        NoRepeat => LayoutPrimitiveInfo::with_clip_rect(
            info.rect,
            TypedRect::new(
                info.rect.origin,
                TypedSize2D::new(background_size.width, background_size.height),
            ),
        ),
        Repeat => *info,
        RepeatX => LayoutPrimitiveInfo::with_clip_rect(
            info.rect,
            TypedRect::new(
                info.rect.origin,
                TypedSize2D::new(info.rect.size.width, background_size.height),
            ),
        ),
        RepeatY => LayoutPrimitiveInfo::with_clip_rect(
            info.rect,
            TypedRect::new(
                info.rect.origin,
                TypedSize2D::new(background_size.width, info.rect.size.height),
            ),
        ),
    }
}

struct Ratio {
    width: f32,
    height: f32
}

fn calculate_background_size(
    bg_size: &StyleBackgroundSize,
    info: &PrimitiveInfo<LayoutPixel>,
    image_dimensions: &(i32, i32)
) -> TypedSize2D<f32, LayoutPixel> {

    let original_ratios = Ratio {
        width: info.rect.size.width / image_dimensions.0 as f32,
        height: info.rect.size.height / image_dimensions.1 as f32,
    };

    let ratio = match bg_size {
        StyleBackgroundSize::Contain => original_ratios.width.min(original_ratios.height),
        StyleBackgroundSize::Cover => original_ratios.width.max(original_ratios.height)
    };

    TypedSize2D::new(image_dimensions.0 as f32 * ratio, image_dimensions.1 as f32 * ratio)
}

#[inline]
fn push_image(
    info: &PrimitiveInfo<LayoutPixel>,
    builder: &mut DisplayListBuilder,
    app_resources: &AppResources,
    image_id: &ImageId,
    size: TypedSize2D<f32, LayoutPixel>
) {
    use wr_translate::translate_image_key;
    if let Some(image_info) = app_resources.get_image_info(image_id) {
        builder.push_image(
            info,
            size,
            LayoutSize::zero(),
            ImageRendering::Auto,
            AlphaType::PremultipliedAlpha,
            translate_image_key(image_info.key),
            ColorF::WHITE,
        );
    }
}

#[inline]
fn push_border(
    info: &PrimitiveInfo<LayoutPixel>,
    builder: &mut DisplayListBuilder,
    border: &StyleBorder,
    border_radius: &Option<StyleBorderRadius>)
{
    use css::webrender_translate::{
        wr_translate_layout_side_offsets, wr_translate_border_details
    };

    if let Some((border_widths, border_details)) = border.get_webrender_border(*border_radius) {
        builder.push_border(
            info,
            wr_translate_layout_side_offsets(border_widths),
            wr_translate_border_details(border_details));
    }
}

/// Subtracts the padding from the bounds, returning the new bounds
///
/// Warning: The resulting rectangle may have negative width or height
fn subtract_padding(bounds: &TypedRect<f32, LayoutPixel>, padding: &LayoutPadding)
-> TypedRect<f32, LayoutPixel>
{
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
