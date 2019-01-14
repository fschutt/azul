#![allow(unused_variables)]
#![allow(unused_macros)]

use std::{
    fmt,
    sync::{Arc, Mutex},
    collections::BTreeMap,
};
use app_units::{AU_PER_PX, MIN_AU, MAX_AU, Au};
use euclid::{TypedRect, TypedSize2D};
use glium::glutin::dpi::{LogicalPosition, LogicalSize};
use webrender::api::{
    LayoutPixel, RenderApi, FontInstanceKey,
    DisplayListBuilder, PrimitiveInfo, GradientStop, ColorF, PipelineId, Epoch,
    ImageData, ImageKey, ImageDescriptor, ResourceUpdate, AddImage, AddFontInstance,
    AddFont, BorderRadius, ClipMode, LayoutPoint, LayoutSize,
    GlyphOptions, LayoutRect, BorderSide, FontKey, ExternalScrollId,
    NormalBorder, ComplexClipRegion, LayoutPrimitiveInfo, ExternalImageId,
    ExternalImageData, ImageFormat, ExternalImageType, TextureTarget,
    ImageRendering, AlphaType, FontInstanceFlags, FontRenderMode, BorderDetails,
    ColorU, BorderStyle,
};
use azul_css::{
    Css, StyleTextAlignmentHorz, LayoutPosition,CssProperty, LayoutOverflow,
    StyleFontSize, StyleBorderRadius, PixelValue, FloatValue, LayoutMargin,
    StyleTextColor, StyleBackground, StyleBoxShadow, StyleBackgroundColor,
    StyleBackgroundSize, StyleBackgroundRepeat, StyleBorder, BoxShadowPreDisplayItem,
    LayoutPadding, SizeMetric, BoxShadowClipMode, FontId, StyleTextAlignmentVert,
    RectStyle, RectLayout, ColorU as StyleColorU
};
use {
    FastHashMap,
    app_resources::AppResources,
    default_callbacks::StackCheckedPointer,
    traits::Layout,
    ui_state::UiState,
    ui_description::{UiDescription, StyledNode},
    id_tree::{NodeDataContainer, NodeId, NodeHierarchy},
    dom::{
        IFrameCallback, NodeData, GlTextureCallback, ScrollTagId, DomHash, new_scroll_tag_id,
        NodeType::{self, Div, Text, Image, GlTexture, IFrame, Label}
    },
    text_layout::{TextOverflowPass2, ScrollbarInfo, Words, FontMetrics},
    images::ImageId,
    text_cache::TextInfo,
    compositor::new_opengl_texture_id,
    window::{Window, WindowInfo, FakeWindow, ScrollStates, HidpiAdjustedBounds},
    window_state::WindowSize,
};

const DEFAULT_FONT_COLOR: StyleTextColor = StyleTextColor(StyleColorU { r: 0, b: 0, g: 0, a: 255 });

// In case no font size is specified for a node,
// this will be substituted as the default font size
lazy_static! {
    pub static ref DEFAULT_FONT_SIZE: StyleFontSize = StyleFontSize(PixelValue {
        metric: SizeMetric::Px,
        number: FloatValue::new(100.0),
    });
}

pub(crate) struct DisplayList<'a, T: Layout + 'a> {
    pub(crate) ui_descr: &'a UiDescription<T>,
    pub(crate) rectangles: NodeDataContainer<DisplayRectangle<'a>>
}

impl<'a, T: Layout + 'a> fmt::Debug for DisplayList<'a, T> {
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
        Self {
            tag: tag,
            styled_node: styled_node,
            style: RectStyle::default(),
            layout: RectLayout::default(),
        }
    }
}

impl<'a, T: Layout + 'a> DisplayList<'a, T> {

    /// NOTE: This function assumes that the UiDescription has an initialized arena
    ///
    /// This only looks at the user-facing styles of the `UiDescription`, not the actual
    /// layout. The layout is done only in the `into_display_list_builder` step.
    pub(crate) fn new_from_ui_description(ui_description: &'a UiDescription<T>, ui_state: &UiState<T>) -> Self {
        let arena = ui_description.ui_descr_arena.borrow();

        let display_rect_arena = arena.node_data.transform(|node, node_id| {
            let style = ui_description.styled_nodes.get(&node_id).unwrap_or(&ui_description.default_style_of_node);
            let tag = ui_state.node_ids_to_tag_ids.get(&node_id).and_then(|tag| Some(*tag));
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
        app_resources: &mut AppResources)
    -> (DisplayListBuilder, ScrolledNodes)
    {
        use glium::glutin::dpi::LogicalSize;

        let mut resource_updates = Vec::<ResourceUpdate>::new();
        let arena = self.ui_descr.ui_descr_arena.borrow();
        let node_hierarchy = &arena.node_layout;
        let node_data = &arena.node_data;

        // Upload image and font resources
        update_resources(&window.internal.api, app_resources, &mut resource_updates);

        let (laid_out_rectangles, node_depths, word_cache) = do_the_layout(
            node_hierarchy,
            node_data,
            &self.rectangles,
            &mut resource_updates,
            app_resources,
            &window.internal.api,
            window.state.size.dimensions,
            LogicalPosition::new(0.0, 0.0)
        );

        let mut scrollable_nodes = get_nodes_that_need_scroll_clip(
            node_hierarchy, &self.rectangles, node_data, &laid_out_rectangles,
            &node_depths, window.internal.pipeline_id
        );

        // Make sure unused scroll states are garbage collected.
        window.scroll_states.remove_unused_scroll_states();

        let LogicalSize { width, height } = window.state.size.dimensions;
        let mut builder = DisplayListBuilder::with_capacity(window.internal.pipeline_id, TypedSize2D::new(width as f32, height as f32), self.rectangles.len());

        let rects_in_rendering_order = determine_rendering_order(node_hierarchy, &self.rectangles, &laid_out_rectangles);

        push_rectangles_into_displaylist(
            &laid_out_rectangles,
            window.internal.epoch,
            window.state.size,
            rects_in_rendering_order,
            &mut scrollable_nodes,
            &mut window.scroll_states,
            &DisplayListParametersRef {
                pipeline_id: window.internal.pipeline_id,
                node_hierarchy: node_hierarchy,
                node_data: node_data,
                render_api: &window.internal.api,
                display_rectangle_arena: &self.rectangles,
                css: &window.css,
                word_cache: &word_cache,
            },
            &mut DisplayListParametersMut {
                app_data: app_data_access,
                app_resources,
                fake_window,
                builder: &mut builder,
                resource_updates: &mut resource_updates,
                pipeline_id: window.internal.pipeline_id,
            },
        );

        &window.internal.api.update_resources(resource_updates);

        (builder, scrollable_nodes)
    }
}

/// Looks if any new images need to be uploaded and stores the in the image resources
fn update_resources(
    api: &RenderApi,
    app_resources: &mut AppResources,
    resource_updates: &mut Vec<ResourceUpdate>)
{
    update_image_resources(api, app_resources, resource_updates);
    update_font_resources(api, app_resources, resource_updates);
}

fn update_image_resources(
    api: &RenderApi,
    app_resources: &mut AppResources,
    resource_updates: &mut Vec<ResourceUpdate>)
{
    use images::{ImageState, ImageInfo};

    let mut updated_images = Vec::<(ImageId, (ImageData, ImageDescriptor))>::new();
    let mut to_delete_images = Vec::<(ImageId, Option<ImageKey>)>::new();

    // possible performance bottleneck (duplicated cloning) !!
    for (key, value) in app_resources.images.iter() {
        match *value {
            ImageState::ReadyForUpload(ref d) => {
                updated_images.push((key.clone(), d.clone()));
            },
            ImageState::Uploaded(_) => { },
            ImageState::AboutToBeDeleted((ref k, _)) => {
                to_delete_images.push((key.clone(), k.clone()));
            }
        }
    }

    // Remove any images that should be deleted
    for (resource_key, image_key) in to_delete_images.into_iter() {
        if let Some(image_key) = image_key {
            resource_updates.push(ResourceUpdate::DeleteImage(image_key));
        }
        app_resources.images.remove(&resource_key);
    }

    // Upload all remaining images to the GPU only if the haven't been
    // uploaded yet
    for (resource_key, (data, descriptor)) in updated_images.into_iter() {

        let key = api.generate_image_key();
        resource_updates.push(ResourceUpdate::AddImage(
            AddImage { key, descriptor, data, tiling: None }
        ));

        *app_resources.images.get_mut(&resource_key).unwrap() =
            ImageState::Uploaded(ImageInfo {
                key: key,
                descriptor: descriptor
        });
    }
}

// almost the same as update_image_resources, but fonts
// have two HashMaps that need to be updated
fn update_font_resources(
    api: &RenderApi,
    app_resources: &mut AppResources,
    resource_updates: &mut Vec<ResourceUpdate>)
{
    use font::FontState;
    use azul_css::FontId;

    let mut updated_fonts = Vec::<(FontId, Vec<u8>)>::new();
    let mut to_delete_fonts = Vec::<(FontId, Option<(FontKey, Vec<FontInstanceKey>)>)>::new();

    for (key, value) in app_resources.font_data.borrow().iter() {
        match &*(*value.2).borrow() {
            FontState::ReadyForUpload(ref bytes) => {
                updated_fonts.push((key.clone(), bytes.clone()));
            },
            FontState::Uploaded(_) => { },
            FontState::AboutToBeDeleted(ref font_key) => {
                let to_delete_font_instances = font_key.and_then(|f_key| {
                    let to_delete_font_instances = app_resources.fonts[&f_key].values().cloned().collect();
                    Some((f_key.clone(), to_delete_font_instances))
                });
                to_delete_fonts.push((key.clone(), to_delete_font_instances));
            }
        }
    }

    // Delete the complete font. Maybe a more granular option to
    // keep the font data in memory should be added later
    for (resource_key, to_delete_instances) in to_delete_fonts.into_iter() {
        if let Some((font_key, font_instance_keys)) = to_delete_instances {
            for instance in font_instance_keys {
                resource_updates.push(ResourceUpdate::DeleteFontInstance(instance));
            }
            resource_updates.push(ResourceUpdate::DeleteFont(font_key));
            app_resources.fonts.remove(&font_key);
        }
        app_resources.font_data.borrow_mut().remove(&resource_key);
    }

    // Upload all remaining fonts to the GPU only if the haven't been uploaded yet
    for (resource_key, data) in updated_fonts.into_iter() {
        let key = api.generate_font_key();
        resource_updates.push(ResourceUpdate::AddFont(AddFont::Raw(key, data, 0))); // TODO: use the index better?
        let mut borrow_mut = app_resources.font_data.borrow_mut();
        *borrow_mut.get_mut(&resource_key).unwrap().2.borrow_mut() = FontState::Uploaded(key);
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
    layouted_rects: &NodeDataContainer<LayoutRect>,
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
    layouted_rects: &NodeDataContainer<LayoutRect>,
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
            clip_children: node_needs_to_clip_children(&rectangles[root_id].style),
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
                        let node_is_overflow_hidden = node_needs_to_clip_children(&rect_node.style);
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

#[derive(Debug, Clone)]
pub struct WordCache(BTreeMap<NodeId, (Words, FontMetrics)>);

fn do_the_layout<'a,'b, T: Layout>(
    node_hierarchy: &NodeHierarchy,
    node_data: &NodeDataContainer<NodeData<T>>,
    display_rects: &NodeDataContainer<DisplayRectangle<'a>>,
    resource_updates: &mut Vec<ResourceUpdate>,
    app_resources: &'b mut AppResources,
    render_api: &RenderApi,
    rect_size: LogicalSize,
    rect_offset: LogicalPosition)
-> (NodeDataContainer<LayoutRect>, Vec<(usize, NodeId)>, WordCache)
{
    use text_layout::{split_text_into_words, get_words_cached};
    use ui_solver::{solve_flex_layout_height, solve_flex_layout_width, get_x_positions, get_y_positions};

    let word_cache: BTreeMap<NodeId, (Words, FontMetrics)> = node_hierarchy
    .linear_iter()
    .filter_map(|id| {
        let (font, font_metrics, font_id, font_size) = match node_data[id].node_type {
            NodeType::Label(_) | NodeType::Text(_) => {
                use text_layout::TextLayoutOptions;

                let rect = &display_rects[id];
                let style = &rect.style;
                let font_id = style.font_family.as_ref()?.fonts.get(0)?.clone();
                let font_size = style.font_size.unwrap_or(*DEFAULT_FONT_SIZE);
                let font_size_app_units = Au((font_size.0.to_pixels() as i32) * AU_PER_PX as i32);
                let font_instance_key = push_font(&font_id, font_size_app_units, resource_updates, app_resources, render_api)?;
                let overflow_behaviour = style.overflow.unwrap_or(LayoutOverflow::default());
                let font = app_resources.get_font(&font_id)?;
                let (horz_alignment, vert_alignment) = determine_text_alignment(rect);

                let text_layout_options = TextLayoutOptions {
                    horz_alignment,
                    vert_alignment,
                    line_height: style.line_height,
                    letter_spacing: style.letter_spacing,
                };
                let font_metrics = FontMetrics::new(&font.0, &font_size, &text_layout_options);

                (font.0, font_metrics, font_id, font_size)
            },
            _ => return None,
        };

        match &node_data[id].node_type {
            NodeType::Label(ref string_to_render) => {
                Some((id, (split_text_into_words(&string_to_render, &font, font_metrics.font_size_no_line_height, font_metrics.letter_spacing), font_metrics)))
            },
            NodeType::Text(text_id) => {
                // Cloning the words here due to lifetime problems
                Some((id, (get_words_cached(&text_id,
                    &font,
                    &font_id,
                    &font_size,
                    font_metrics.font_size_no_line_height,
                    font_metrics.letter_spacing,
                    &mut app_resources.text_cache).clone(), font_metrics)))
            },
            _ => None,
        }
    }).collect();

    let preferred_widths = node_data.transform(|node, _| {
        node.node_type.get_preferred_width(&app_resources.images)
    });

    let solved_widths = solve_flex_layout_width(
        node_hierarchy,
        &display_rects,
        preferred_widths,
        rect_size.width as f32,
    );

    let preferred_heights = node_data.transform(|node, id| {
        use text_layout::TextSizePx;
        node.node_type.get_preferred_height_based_on_width(
            TextSizePx(solved_widths.solved_widths[id].total()),
            &app_resources.images,
            word_cache.get(&id).and_then(|e| Some(&e.0)),
            word_cache.get(&id).and_then(|e| Some(e.1)),
        ).and_then(|text_size| Some(text_size.0))
    });

    let solved_heights = solve_flex_layout_height(
        node_hierarchy,
        &solved_widths,
        preferred_heights,
        rect_size.height as f32,
    );

    let x_positions = get_x_positions(&solved_widths, node_hierarchy, rect_offset);
    let y_positions = get_y_positions(&solved_heights, &solved_widths, node_hierarchy, rect_offset);

    let layouted_arena = node_data.transform(|node, node_id| {
        LayoutRect::new(
            LayoutPoint::new(x_positions[node_id].0, y_positions[node_id].0),
            LayoutSize::new(
                solved_widths.solved_widths[node_id].total(),
                solved_heights.solved_heights[node_id].total(),
            )
        )
    });

    (layouted_arena, solved_widths.non_leaf_nodes_sorted_by_depth, WordCache(word_cache))
}

#[derive(Default, Debug, Clone)]
pub(crate)  struct ScrolledNodes {
    pub(crate) overflowing_nodes: BTreeMap<NodeId, OverflowingScrollNode>,
    pub(crate) tags_to_node_ids: BTreeMap<ScrollTagId, NodeId>,
}

#[derive(Debug, Clone)]
pub(crate) struct OverflowingScrollNode {
    pub(crate) parent_rect: LayoutRect,
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
fn get_nodes_that_need_scroll_clip<'a, T: 'a + Layout>(
    node_hierarchy: &NodeHierarchy,
    display_list_rects: &NodeDataContainer<DisplayRectangle<'a>>,
    dom_rects: &NodeDataContainer<NodeData<T>>,
    layouted_rects: &NodeDataContainer<LayoutRect>,
    parents: &[(usize, NodeId)],
    pipeline_id: PipelineId,
) -> ScrolledNodes {

    let mut nodes = BTreeMap::new();
    let mut tags_to_node_ids = BTreeMap::new();

    for (_, parent) in parents {

        let mut children_sum_rect = None;

        for child in parent.children(&node_hierarchy) {
            let old = children_sum_rect.unwrap_or(LayoutRect::zero());
            children_sum_rect = Some(old.union(&layouted_rects[child]));
        }

        let children_sum_rect = match children_sum_rect {
            None => continue,
            Some(sum) => sum,
        };

        let parent_rect = &layouted_rects.get(*parent).unwrap();

        if children_sum_rect.contains_rect(parent_rect) {
            continue;
        }

        let parent_dom_hash = dom_rects[*parent].calculate_node_data_hash();

        // Create an external scroll id. This id is required to preserve its
        // scroll state accross multiple frames.
        let parent_external_scroll_id  = ExternalScrollId(parent_dom_hash.0, pipeline_id);

        // Create a unique scroll tag for hit-testing
        let scroll_tag_id = match display_list_rects.get(*parent).and_then(|node| node.tag) {
            Some(existing_tag) => ScrollTagId(existing_tag),
            None => new_scroll_tag_id(),
        };

        tags_to_node_ids.insert(scroll_tag_id, *parent);
        nodes.insert(*parent, OverflowingScrollNode {
            parent_rect: *parent_rect.clone(),
            child_rect: children_sum_rect,
            parent_external_scroll_id,
            parent_dom_hash,
            scroll_tag_id,
        });
    }

    ScrolledNodes { overflowing_nodes: nodes, tags_to_node_ids }
}

fn node_needs_to_clip_children(style: &RectStyle) -> bool {
    let overflow = style.overflow.unwrap_or_default();
    overflow.horizontal.clips_children() ||
    overflow.vertical.clips_children()
}

#[test]
fn test_overflow_parsing() {
    use azul_css::{TextOverflowBehaviour, TextOverflowBehaviourInner};
    let style1 = RectStyle::default();
    assert!(!node_needs_to_clip_children(&style1));

    let style2 = RectStyle {
        overflow: Some(LayoutOverflow {
            horizontal: TextOverflowBehaviour::Modified(TextOverflowBehaviourInner::Visible),
            vertical: TextOverflowBehaviour::Modified(TextOverflowBehaviourInner::Visible),
        }),
        .. Default::default()
    };
    assert!(!node_needs_to_clip_children(&style2));

    let style3 = RectStyle {
        overflow: Some(LayoutOverflow {
            horizontal: TextOverflowBehaviour::Modified(TextOverflowBehaviourInner::Hidden),
            vertical: TextOverflowBehaviour::Modified(TextOverflowBehaviourInner::Visible),
        }),
        .. Default::default()
    };
    assert!(node_needs_to_clip_children(&style3));
}

fn push_rectangles_into_displaylist<'a, 'b, 'c, 'd, 'e, 'f, T: Layout>(
    solved_rects: &NodeDataContainer<LayoutRect>,
    epoch: Epoch,
    window_size: WindowSize,
    content_grouped_rectangles: ContentGroupOrder,
    scrollable_nodes: &mut ScrolledNodes,
    scroll_states: &mut ScrollStates,
    referenced_content: &DisplayListParametersRef<'a,'b,'c,'d,'e, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T>)
{
/* -- disabled scrolling temporarily due to z-indexing problems
    // A stack containing all the nodes which have a scroll clip pushed to the builder.
    let mut stack: Vec<NodeId> = vec![];
*/

    let mut clip_stack = Vec::new();

    for content_group in content_grouped_rectangles.groups {
        // Push the root of the node
        push_rectangles_into_displaylist_inner(content_group.root,
            solved_rects,
            epoch,
            scrollable_nodes,
            referenced_content,
            referenced_mutable_content,
            &mut clip_stack
        );

        for item in content_group.node_ids {
            push_rectangles_into_displaylist_inner(
                item,
                solved_rects,
                epoch,
                scrollable_nodes,
                referenced_content,
                referenced_mutable_content,
                &mut clip_stack
            );
        }
    }
/*
    for (z_index, rects) in z_ordered_rectangles.0.into_iter() {
        for rect_idx in rects {
            let html_node = &arena[rect_idx];
            let rectangle = DisplayListRectParams {
                epoch,
                rect_idx,
                html_node: &html_node.node_type,
            };

            let styled_node = &referenced_content.display_rectangle_arena[rect_idx];
            let solved_rect = solved_rects[rect_idx];

            displaylist_handle_rect(solved_rect, scrollable_nodes, rectangle, referenced_content, referenced_mutable_content);
/*
            // If the current node is a parent that has overflow:hidden set, push
            // the clip ID and the last child into the stack
            if html_node.last_child.is_some() {
                if node_needs_to_clip_children(&styled_node.style) {

                }
            }

            // If the current node is the last child of the parent and the parent has
            // overflow:hidden set, pop the last clip id
            if clip_stack.last().cloned() == Some(rect_idx) {
                referenced_mutable_content.builder.pop_clip_id();
                clip_stack.pop();
            }
*/
/* -- disabled scrolling temporarily due to z-indexing problems
            if let Some(OverflowingScrollNode { parent_external_scroll_id, parent_rect, child_rect, .. }) = scrollable_nodes.overflowing_nodes.get(&rect_idx) {

                // The unwraps on the following line must succeed, as if we have no children, we can't have a scrollable content.
                stack.push(rect_idx.children(&arena).last().unwrap());

                // Create a new scroll state for each node that is not present in the scroll states already.
                // The arena containing the actual dom maps 1:1 to the arena containing the rectangles, so we
                // can use the NodeIds from the layouted rectangles to access the NodeData corresponding
                // to each Rectangle in the NodeData arena.
                //
                // This next unwrap is fine since we are sure the looked up NodeId exists in the arena!
                scroll_states.ensure_initialized_scroll_state(
                    *parent_external_scroll_id,
                    child_rect.size.width - parent_rect.size.width,
                    child_rect.size.height - parent_rect.size.height
                );

                // Set the scrolling clip
                let clip_id = referenced_mutable_content.builder.define_scroll_frame(
                    Some(*parent_external_scroll_id),
                    *child_rect,
                    *parent_rect,
                    vec![],
                    None,
                    ScrollSensitivity::ScriptAndInputEvents,
                );

                referenced_mutable_content.builder.push_clip_id(clip_id);
            }

            if let Some(&child_idx) = stack.last() {
                if child_idx == rect_idx {
                    stack.pop();
                    referenced_mutable_content.builder.pop_clip_id();
                }
            }
*/
        }
    }
*/
}

fn push_rectangles_into_displaylist_inner<'a,'b,'c,'d,'e,'f, T: Layout>(
    item: RenderableNodeId,
    solved_rects_data: &NodeDataContainer<LayoutRect>,
    epoch: Epoch,
    scrollable_nodes: &mut ScrolledNodes,
    referenced_content: &DisplayListParametersRef<'a,'b,'c,'d,'e, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'f, T>,
    clip_stack: &mut Vec<NodeId>)
{
    let html_node = &referenced_content.node_data[item.node_id];
    let solved_rect = solved_rects_data[item.node_id];
    let rectangle = DisplayListRectParams {
        epoch,
        rect_idx: item.node_id,
        html_node: &html_node.node_type,
    };

    displaylist_handle_rect(
        solved_rect,
        scrollable_nodes,
        rectangle,
        referenced_content,
        referenced_mutable_content
    );

    if item.clip_children {
        if let Some(last_child) = referenced_content.node_hierarchy[item.node_id].last_child {
            let styled_node = &referenced_content.display_rectangle_arena[item.node_id];
            let solved_rect = solved_rects_data[item.node_id];
            let clip = get_clip_region(solved_rect, &styled_node)
                .unwrap_or(ComplexClipRegion::new(solved_rect, BorderRadius::zero(), ClipMode::Clip));
            let clip_id = referenced_mutable_content.builder.define_clip(solved_rect, vec![clip], /* image_mask: */ None);
            referenced_mutable_content.builder.push_clip_id(clip_id);
            clip_stack.push(last_child);
        }
    }

    if clip_stack.last().cloned() == Some(item.node_id) {
        referenced_mutable_content.builder.pop_clip_id();
        clip_stack.pop();
    }
}

/// Parameters that apply to a single rectangle / div node
#[derive(Copy, Clone)]
pub(crate) struct DisplayListRectParams<'a, T: 'a + Layout> {
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
fn displaylist_handle_rect<'a,'b,'c,'d,'e,'f,'g, T: Layout>(
    bounds: LayoutRect,
    scrollable_nodes: &mut ScrolledNodes,
    rectangle: DisplayListRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'b,'c,'d,'e,'f, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'g, T>)
{
    use text_layout::{TextOverflow, TextSizePx};
    use webrender::api::BorderRadius;

    let DisplayListParametersRef {
        render_api, css,
        display_rectangle_arena, word_cache, pipeline_id,
        node_hierarchy, node_data,
    } = referenced_content;

    let DisplayListRectParams {
        epoch, rect_idx, html_node, window_size,
    } = rectangle;

    let rect = &display_rectangle_arena[rect_idx];

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
    if let Some(bg_col) = &rect.style.background_color {
        // The background color won't be seen anyway, so don't push a
        // background color if we do have a background already
        if rect.style.background.is_none() {
            push_rect(
                &info,
                referenced_mutable_content.builder,
                bg_col,
            );
        }
    } else if info.tag.is_some() {
        const TRANSPARENT_BG: StyleBackgroundColor = StyleBackgroundColor(StyleColorU { r: 0, g: 0, b: 0, a: 0 });
        push_rect(
            &info,
            referenced_mutable_content.builder,
            &TRANSPARENT_BG,
        );
    }

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
    }

    if let Some(ref border) = rect.style.border {
        push_border(
            &info,
            referenced_mutable_content.builder,
            &border,
            &rect.style.border_radius,
        );
    }

    let (horz_alignment, vert_alignment) = determine_text_alignment(rect);

    let scrollbar_style = ScrollbarInfo {
        width: TextSizePx(17.0),
        padding: TextSizePx(2.0),
        background_color: StyleBackgroundColor(StyleColorU { r: 241, g: 241, b: 241, a: 255 }),
        triangle_color: StyleBackgroundColor(StyleColorU { r: 163, g: 163, b: 163, a: 255 }),
        bar_color: StyleBackgroundColor(StyleColorU { r: 193, g: 193, b: 193, a: 255 }),
    };

    // The only thing changed between TextId and String is
    //`TextInfo::Cached` vs `TextInfo::Uncached` - reduce code duplication
    let push_text_wrapper = |
        text_info: &TextInfo,
        builder: &mut DisplayListBuilder,
        app_resources: &mut AppResources,
        resource_updates: &mut Vec<ResourceUpdate>|
    {
        let words = word_cache.0.get(&rect_idx)?;

        // Adjust the bounds by the padding
        let mut text_bounds = rect.layout.padding
            .as_ref()
            .map(|padding| subtract_padding(&bounds, padding))
            .unwrap_or(bounds);

        text_bounds.size.width = text_bounds.size.width.max(0.0);
        text_bounds.size.height = text_bounds.size.height.max(0.0);

        let text_clip_region_id = rect.layout.padding.map(|_|
            builder.define_clip(text_bounds, vec![ComplexClipRegion {
                rect: text_bounds,
                radii: BorderRadius::zero(),
                mode: ClipMode::Clip,
            }], None)
        );

        if let Some(text_clip_id) = text_clip_region_id {
            builder.push_clip_id(text_clip_id);
        }

        let overflow = push_text(
            &info,
            text_info,
            builder,
            &rect.style,
            app_resources,
            &render_api,
            &text_bounds,
            resource_updates,
            horz_alignment,
            vert_alignment,
            &scrollbar_style,
            &words.0);

        if text_clip_region_id.is_some() {
            builder.pop_clip_id();
        }

        overflow
    };

    // Handle the special content of the node, return if it overflows in the vertical direction
    let overflow_result = match html_node {
        Div => { None },
        Label(text) => push_text_wrapper(
            &TextInfo::Uncached(text.clone()),
            referenced_mutable_content.builder,
            referenced_mutable_content.app_resources,
            referenced_mutable_content.resource_updates),
        Text(text_id) => push_text_wrapper(
            &TextInfo::Cached(*text_id),
            referenced_mutable_content.builder,
            referenced_mutable_content.app_resources,
            referenced_mutable_content.resource_updates),
        Image(image_id) => push_image(
            &info,
            referenced_mutable_content.builder,
            referenced_mutable_content.app_resources,
            image_id,
            info.rect.size),
        GlTexture(callback) => push_opengl_texture(callback, &info, rectangle, referenced_content, referenced_mutable_content),
        IFrame(callback) => push_iframe(callback, &info, scrollable_nodes, rectangle, referenced_content, referenced_mutable_content),
    };

    // Push the inset shadow (if any)
    push_box_shadow(
        referenced_mutable_content.builder,
        &rect.style,
        &bounds,
        BoxShadowClipMode::Inset);

    // Push scrollbars if necessary
    if let Some(overflow) = &overflow_result {
        // If the rectangle should have a scrollbar, push a scrollbar onto the display list
        if rect.style.overflow.unwrap_or_default().allows_vertical_scrollbar() {
            if let TextOverflow::IsOverflowing(amount_vert) = overflow.text_overflow.vertical {
                push_scrollbar(referenced_mutable_content.builder, &overflow.text_overflow, &scrollbar_style, &bounds, &rect.style.border)
            }
        }
        if rect.style.overflow.unwrap_or_default().allows_horizontal_scrollbar() {
            if let TextOverflow::IsOverflowing(amount_horz) = overflow.text_overflow.horizontal {
                push_scrollbar(referenced_mutable_content.builder, &overflow.text_overflow, &scrollbar_style, &bounds, &rect.style.border)
            }
        }
    }

    if clip_region_id.is_some() {
        referenced_mutable_content.builder.pop_clip_id();
    }
}

fn push_opengl_texture<'a,'b,'c,'d,'e,'f,'g, T: Layout>(
    (texture_callback, texture_stack_ptr): &(GlTextureCallback<T>, StackCheckedPointer<T>),
    info: &LayoutPrimitiveInfo,
    rectangle: DisplayListRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'b,'c,'d,'e,'f, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'g, T>,
) -> Option<OverflowInfo>
{
    use compositor::{ActiveTexture, ACTIVE_GL_TEXTURES};
    use gleam::gl;

    let bounds = HidpiAdjustedBounds::from_bounds(info.rect, rectangle.window_size.hidpi_factor);

    let texture;

    {
        // Make sure that the app data is locked before invoking the callback
        let _lock = referenced_mutable_content.app_data.lock().unwrap();
        texture = (texture_callback.0)(&texture_stack_ptr, LayoutInfo {
            window: &mut *referenced_mutable_content.fake_window,
            resources: &referenced_mutable_content.app_resources,
        }, bounds);

        // Reset the framebuffer and SRGB color target to 0
        let gl_context = referenced_mutable_content.fake_window.read_only_window().get_gl_context();

        gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
        gl_context.disable(gl::FRAMEBUFFER_SRGB);
    }

    let texture = texture?;

    let opaque = false;
    let allow_mipmaps = true;
    let descriptor = ImageDescriptor::new(info.rect.size.width as i32, info.rect.size.height as i32, ImageFormat::BGRA8, opaque, allow_mipmaps);
    let key = referenced_content.render_api.generate_image_key();
    let external_image_id = ExternalImageId(new_opengl_texture_id() as u64);

    let data = ImageData::External(ExternalImageData {
        id: external_image_id,
        channel_index: 0,
        image_type: ExternalImageType::TextureHandle(TextureTarget::Default),
    });

    ACTIVE_GL_TEXTURES.lock().unwrap()
        .entry(rectangle.epoch).or_insert_with(|| FastHashMap::default())
        .insert(external_image_id, ActiveTexture { texture: texture.clone() });

    referenced_mutable_content.resource_updates.push(ResourceUpdate::AddImage(
        AddImage { key, descriptor, data, tiling: None }
    ));

    referenced_mutable_content.builder.push_image(
        &info,
        LayoutSize::new(texture.inner.width() as f32, texture.inner.height() as f32),
        LayoutSize::zero(),
        ImageRendering::Auto,
        AlphaType::Alpha,
        key,
        ColorF::WHITE);

    None
}

fn push_iframe<'a,'b,'c,'d,'e,'f,'g, T: Layout>(
    (iframe_callback, iframe_pointer): &(IFrameCallback<T>, StackCheckedPointer<T>),
    info: &LayoutPrimitiveInfo,
    parent_scrollable_nodes: &mut ScrolledNodes,
    rectangle: DisplayListRectParams<'a, T>,
    referenced_content: &DisplayListParametersRef<'b,'c,'d,'e,'f, T>,
    referenced_mutable_content: &mut DisplayListParametersMut<'g, T>,
) -> Option<OverflowInfo>
{
    use glium::glutin::dpi::{LogicalPosition, LogicalSize};

    let bounds = HidpiAdjustedBounds::from_bounds(info.rect, rectangle.window_size.hidpi_factor);

    let new_dom;

    {
        // Make sure that the app data is locked before invoking the callback
        let _lock = referenced_mutable_content.app_data.lock().unwrap();

        let window_info = LayoutInfo {
            window: referenced_mutable_content.fake_window,
            resources: &referenced_mutable_content.app_resources,
        };
        new_dom = (iframe_callback.0)(&iframe_pointer, window_info, bounds);
    }

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

    let arena = ui_description.ui_descr_arena.borrow();
    let node_hierarchy = &arena.node_layout;
    let node_data = &arena.node_data;

    // Insert the DOM into the solver so we can solve the layout of the rectangles
    let rect_size = LogicalSize::new(info.rect.size.width as f64, info.rect.size.height as f64);
    let rect_origin = LogicalPosition::new(info.rect.origin.x as f64, info.rect.origin.y as f64);

    let (laid_out_rectangles, node_depths, word_cache) = do_the_layout(
        &node_hierarchy,
        &node_data,
        &display_list.rectangles,
        &mut referenced_mutable_content.resource_updates,
        &mut referenced_mutable_content.app_resources,
        &referenced_content.render_api,
        rect_size,
        rect_origin);

    let mut scrollable_nodes = get_nodes_that_need_scroll_clip(
        node_hierarchy, &display_list.rectangles, node_data, &laid_out_rectangles,
        &node_depths, referenced_content.pipeline_id);

    let rects_in_rendering_order = determine_rendering_order(node_hierarchy, &display_list.rectangles, &laid_out_rectangles);

    let referenced_content = DisplayListParametersRef {
        // Important: Need to update the ui description, otherwise this function would be endlessly recursive
        node_hierarchy,
        node_data,
        display_rectangle_arena: &display_list.rectangles,
        word_cache: &word_cache,
        .. *referenced_content
    };

    push_rectangles_into_displaylist(
        &laid_out_rectangles,
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

    None
}

/// Since the display list can take a lot of parameters, we don't want to
/// continually pass them as parameters of the function and rather use a
/// struct to pass them around. This is purely for ergonomic reasons.
///
/// `DisplayListParametersRef` has only members that are
///  **immutable references** to other things that need to be passed down the display list
#[derive(Copy, Clone)]
struct DisplayListParametersRef<'a, 'b, 'c, 'd, 'e, T: 'a + Layout> {
    pub pipeline_id: PipelineId,
    pub node_hierarchy: &'e NodeHierarchy,
    pub node_data: &'a NodeDataContainer<NodeData<T>>,
    /// The CSS that should be applied to the DOM
    pub css: &'b Css,
    /// Necessary to push
    pub render_api: &'c RenderApi,
    /// Reference to the arena that contains all the styled rectangles
    pub display_rectangle_arena: &'d NodeDataContainer<DisplayRectangle<'d>>,
    /// Reference to the word cache (left over from the layout,
    /// to re-use the text layout from there)
    pub word_cache: &'c WordCache,
}

/// Same as `DisplayListParametersRef`, but for `&mut Something`
///
/// Note: The `'a` in the `'a + Layout` is technically not required.
/// Only rustc 1.28 requires this, more modern compiler versions insert it automatically.
struct DisplayListParametersMut<'a, T: 'a + Layout> {
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
    pub pipeline_id: PipelineId,
}

#[inline]
fn push_rect(
    info: &PrimitiveInfo<LayoutPixel>,
    builder: &mut DisplayListBuilder,
    color: &StyleBackgroundColor)
{
    use css::webrender_translate::wr_translate_color_u;
    builder.push_rect(&info, wr_translate_color_u(color.0).into());
}

struct OverflowInfo {
    pub text_overflow: TextOverflowPass2,
}

/// Note: automatically pushes the scrollbars on the parent,
/// this should be refined later
#[inline]
fn push_text(
    info: &PrimitiveInfo<LayoutPixel>,
    text: &TextInfo,
    builder: &mut DisplayListBuilder,
    style: &RectStyle,
    app_resources: &mut AppResources,
    render_api: &RenderApi,
    bounds: &TypedRect<f32, LayoutPixel>,
    resource_updates: &mut Vec<ResourceUpdate>,
    horz_alignment: StyleTextAlignmentHorz,
    vert_alignment: StyleTextAlignmentVert,
    scrollbar_info: &ScrollbarInfo,
    words: &Words)
-> Option<OverflowInfo>
{
    use text_layout::{self, TextLayoutOptions};
    use css::webrender_translate::wr_translate_color_u;

    if text.is_empty_text(&*app_resources) {
        return None;
    }

    let font_id = style.font_family.as_ref()?.fonts.get(0)?.clone();
    let font_size = style.font_size.unwrap_or(*DEFAULT_FONT_SIZE);
    let font_size_app_units = Au((font_size.0.to_pixels() as i32) * AU_PER_PX as i32);
    let font_instance_key = push_font(&font_id, font_size_app_units, resource_updates, app_resources, render_api)?;
    let overflow_behaviour = style.overflow.unwrap_or_default();

    let text_layout_options = TextLayoutOptions {
        horz_alignment,
        vert_alignment,
        line_height: style.line_height,
        letter_spacing: style.letter_spacing,
    };

    let (positioned_glyphs, text_overflow) = text_layout::get_glyphs(
        words,
        app_resources,
        bounds,
        &font_id,
        &font_size,
        &text_layout_options,
        text,
        &overflow_behaviour,
        scrollbar_info
    );

    // WARNING: Do not enable FontInstanceFlags::FONT_SMOOTHING or FontInstanceFlags::FORCE_AUTOHINT -
    // they seem to interfere with the text layout thereby messing with the actual text layout.
    let font_color = wr_translate_color_u(style.font_color.unwrap_or(DEFAULT_FONT_COLOR).0).into();
    let mut flags = FontInstanceFlags::empty();
    flags.set(FontInstanceFlags::SUBPIXEL_BGR, true);
    flags.set(FontInstanceFlags::LCD_VERTICAL, true);

    let options = GlyphOptions {
        render_mode: FontRenderMode::Subpixel,
        flags: flags,
    };

    builder.push_text(&info, &positioned_glyphs, font_instance_key, font_color, Some(options));

    Some(OverflowInfo { text_overflow })
}

/// Adds a scrollbar to the left or bottom side of a rectangle.
/// TODO: make styling configurable (like the width / style of the scrollbar)
fn push_scrollbar(
    builder: &mut DisplayListBuilder,
    scrollbar_info: &TextOverflowPass2,
    scrollbar_style: &ScrollbarInfo,
    bounds: &TypedRect<f32, LayoutPixel>,
    border: &Option<StyleBorder>)
{
    use euclid::TypedPoint2D;

    // The border is inside the rectangle - subtract the border width on the left and bottom side,
    // so that the scrollbar is laid out correctly
    let mut bounds = *bounds;
    if let Some(StyleBorder { left: Some(l), bottom: Some(b), .. }) = border {
        bounds.size.width -= l.border_width.to_pixels();
        bounds.size.height -= b.border_width.to_pixels();
    }

    // Background of scrollbar (vertical)
    let scrollbar_vertical_background = TypedRect::<f32, LayoutPixel> {
        origin: TypedPoint2D::new(bounds.origin.x + bounds.size.width - scrollbar_style.width.0, bounds.origin.y),
        size: TypedSize2D::new(scrollbar_style.width.0, bounds.size.height),
    };

    let scrollbar_vertical_background_info = PrimitiveInfo {
        rect: scrollbar_vertical_background,
        clip_rect: bounds,
        is_backface_visible: false,
        tag: None, // TODO: for hit testing
    };

    push_rect(&scrollbar_vertical_background_info, builder, &scrollbar_style.background_color);

    // Actual scroll bar
    let scrollbar_vertical_bar = TypedRect::<f32, LayoutPixel> {
        origin: TypedPoint2D::new(
            bounds.origin.x + bounds.size.width - scrollbar_style.width.0 + scrollbar_style.padding.0,
            bounds.origin.y + scrollbar_style.width.0),
        size: TypedSize2D::new(
            scrollbar_style.width.0 - (scrollbar_style.padding.0 * 2.0),
            bounds.size.height - (scrollbar_style.width.0 * 2.0)),
    };

    let scrollbar_vertical_bar_info = PrimitiveInfo {
        rect: scrollbar_vertical_bar,
        clip_rect: bounds,
        is_backface_visible: false,
        tag: None, // TODO: for hit testing
    };

    push_rect(&scrollbar_vertical_bar_info, builder, &scrollbar_style.bar_color);

    // Triangle top
    let mut scrollbar_triangle_rect = TypedRect::<f32, LayoutPixel> {
        origin: TypedPoint2D::new(
            bounds.origin.x + bounds.size.width - scrollbar_style.width.0 + scrollbar_style.padding.0,
            bounds.origin.y + scrollbar_style.padding.0),
        size: TypedSize2D::new(
            scrollbar_style.width.0 - (scrollbar_style.padding.0 * 2.0),
            scrollbar_style.width.0 - (scrollbar_style.padding.0 * 2.0)),
    };

    scrollbar_triangle_rect.origin.x += scrollbar_triangle_rect.size.width / 4.0;
    scrollbar_triangle_rect.origin.y += scrollbar_triangle_rect.size.height / 4.0;
    scrollbar_triangle_rect.size.width /= 2.0;
    scrollbar_triangle_rect.size.height /= 2.0;

    push_triangle(&scrollbar_triangle_rect, builder, &scrollbar_style.triangle_color, TriangleDirection::PointUp);

    // Triangle bottom
    scrollbar_triangle_rect.origin.y += bounds.size.height - scrollbar_style.width.0 + scrollbar_style.padding.0;
    push_triangle(&scrollbar_triangle_rect, builder, &scrollbar_style.triangle_color, TriangleDirection::PointDown);
}

enum TriangleDirection {
    PointUp,
    PointDown,
    PointRight,
    PointLeft,
}

fn push_triangle(
    bounds: &TypedRect<f32, LayoutPixel>,
    builder: &mut DisplayListBuilder,
    background_color: &StyleBackgroundColor,
    direction: TriangleDirection)
{
    use self::TriangleDirection::*;
    use webrender::api::{LayoutSideOffsets, BorderRadius};
    use css::webrender_translate::wr_translate_color_u;

    // see: https://css-tricks.com/snippets/css/css-triangle/
    // uses the "3d effect" for making a triangle

    let triangle_rect_info = PrimitiveInfo {
        rect: *bounds,
        clip_rect: *bounds,
        is_backface_visible: false,
        tag: None,
    };

    const TRANSPARENT: ColorU = ColorU { r: 0, b: 0, g: 0, a: 0 };

    // make all borders but one transparent
    let [b_left, b_right, b_top, b_bottom] = match direction {
        PointUp         => [
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden),
            (wr_translate_color_u(background_color.0), BorderStyle::Solid)
        ],
        PointDown       => [
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden),
            (wr_translate_color_u(background_color.0), BorderStyle::Solid),
            (TRANSPARENT, BorderStyle::Hidden)
        ],
        PointLeft       => [
            (TRANSPARENT, BorderStyle::Hidden),
            (wr_translate_color_u(background_color.0), BorderStyle::Solid),
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden)
        ],
        PointRight      => [
            (wr_translate_color_u(background_color.0), BorderStyle::Solid),
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden)
        ],
    };

    let border_details = BorderDetails::Normal(NormalBorder {
        left:   BorderSide { color: b_left.0.into(),         style: b_left.1   },
        right:  BorderSide { color: b_right.0.into(),        style: b_right.1  },
        top:    BorderSide { color: b_top.0.into(),          style: b_top.1    },
        bottom: BorderSide { color: b_bottom.0.into(),       style: b_bottom.1 },
        radius: BorderRadius::zero(),
        do_aa: true,
    });

    // make the borders half the width / height of the rectangle,
    // so that the border looks like a triangle
    let left = bounds.size.width / 2.0;
    let top = bounds.size.height / 2.0;
    let bottom = top;
    let right = left;

    let border_widths = LayoutSideOffsets::new(top, right, bottom, left);

    builder.push_border(&triangle_rect_info, border_widths, border_details);
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

    // Box-shadow can be applied to each corner separately. This means, in practice
    // that we simply overlay multiple shadows with shifted clipping rectangles
    let StyleBoxShadow { top, left, bottom, right } = match &style.box_shadow {
        Some(s) => s,
        None => return,
    };
    let border_radius = style.border_radius.unwrap_or(StyleBorderRadius::zero());

    enum ShouldPushShadow {
        PushOneShadow,
        PushTwoShadows,
        PushAllShadows,
    }

    let what_shadow_to_push = match [top, left, bottom, right].iter().filter(|x| x.is_some()).count() {
        1 => ShouldPushShadow::PushOneShadow,
        2 => ShouldPushShadow::PushTwoShadows,
        4 => ShouldPushShadow::PushAllShadows,
        _ => return,
    };

    match what_shadow_to_push {
        ShouldPushShadow::PushOneShadow => {
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
        ShouldPushShadow::PushTwoShadows => {
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
        ShouldPushShadow::PushAllShadows => {

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
            if let Some(image_id) = app_resources.css_ids_to_image_ids.get(&style_image_id.0) {

                let bounds = info.rect;
                let image_dimensions = app_resources.images.get(image_id).and_then(|i| Some(i.get_dimensions()))
                    .unwrap_or((bounds.size.width, bounds.size.height)); // better than crashing...

                let size = match background_size {
                    Some(bg_size) => calculate_background_size(bg_size, &info, &image_dimensions),
                    None => TypedSize2D::new(image_dimensions.0, image_dimensions.1)
                };

                let background_repeat = background_repeat.unwrap_or_default();
                let background_repeat_info = get_background_repeat_info(&info, background_repeat, size);
                push_image(&background_repeat_info, builder, app_resources, image_id, size);
            }
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
    image_dimensions: &(f32, f32)
) -> TypedSize2D<f32, LayoutPixel> {

    let original_ratios = Ratio {
        width: info.rect.size.width / image_dimensions.0,
        height: info.rect.size.height / image_dimensions.1
    };

    let ratio = match bg_size {
        StyleBackgroundSize::Contain => original_ratios.width.min(original_ratios.height),
        StyleBackgroundSize::Cover => original_ratios.width.max(original_ratios.height)
    };

    TypedSize2D::new(image_dimensions.0 * ratio, image_dimensions.1 * ratio)
}

#[inline]
fn push_image(
    info: &PrimitiveInfo<LayoutPixel>,
    builder: &mut DisplayListBuilder,
    app_resources: &AppResources,
    image_id: &ImageId,
    size: TypedSize2D<f32, LayoutPixel>)
-> Option<OverflowInfo>
{
    use images::ImageState::*;

    if let Uploaded(image_info) = app_resources.images.get(image_id)? {
        builder.push_image(
            info,
            size,
            LayoutSize::zero(),
            ImageRendering::Auto,
            AlphaType::PremultipliedAlpha,
            image_info.key,
            ColorF::WHITE,
        );
    }

    // TODO: determine if the image has overflown its container
    None
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

#[inline]
fn push_font(
    font_id: &FontId,
    font_size_app_units: Au,
    resource_updates: &mut Vec<ResourceUpdate>,
    app_resources: &mut AppResources,
    render_api: &RenderApi)
-> Option<FontInstanceKey>
{
    use font::FontState;

    if font_size_app_units < MIN_AU || font_size_app_units > MAX_AU {
        #[cfg(feature = "logging")] {
            error!("warning: too big or too small font size");
        }
        return None;
    }

    let font_state = app_resources.get_font_state(font_id)?;

    let borrow = font_state.borrow();

    match &*borrow {
        FontState::Uploaded(font_key) => {
            let font_sizes_hashmap = app_resources.fonts.entry(*font_key)
                                     .or_insert(FastHashMap::default());
            let font_instance_key = font_sizes_hashmap.entry(font_size_app_units)
                .or_insert_with(|| {
                    let f_instance_key = render_api.generate_font_instance_key();
                    resource_updates.push(ResourceUpdate::AddFontInstance(
                        AddFontInstance {
                            key: f_instance_key,
                            font_key: *font_key,
                            glyph_size: font_size_app_units,
                            options: None,
                            platform_options: None,
                            variations: Vec::new(),
                        }
                    ));
                    f_instance_key
                }
            );

            Some(*font_instance_key)
        },
        _ => {
            // This can happen when the font is loaded for the first time in `.get_font_state`
            // TODO: Make a pre-pass that queries and uploads all non-available fonts
            // error!("warning: trying to use font {:?} that isn't yet available", font_id);
            None
        },
    }
}

/// For a given rectangle, determines what text alignment should be used
fn determine_text_alignment<'a>(rect: &DisplayRectangle<'a>)
-> (StyleTextAlignmentHorz, StyleTextAlignmentVert)
{
    let mut horz_alignment = StyleTextAlignmentHorz::default();
    let mut vert_alignment = StyleTextAlignmentVert::default();

    if let Some(align_items) = rect.layout.align_items {
        // Vertical text alignment
        use azul_css::LayoutAlignItems;
        match align_items {
            LayoutAlignItems::Start => vert_alignment = StyleTextAlignmentVert::Top,
            LayoutAlignItems::End => vert_alignment = StyleTextAlignmentVert::Bottom,
            // technically stretch = blocktext, but we don't have that yet
            _ => vert_alignment = StyleTextAlignmentVert::Center,
        }
    }

    if let Some(justify_content) = rect.layout.justify_content {
        use azul_css::LayoutJustifyContent;
        // Horizontal text alignment
        match justify_content {
            LayoutJustifyContent::Start => horz_alignment = StyleTextAlignmentHorz::Left,
            LayoutJustifyContent::End => horz_alignment = StyleTextAlignmentHorz::Right,
            _ => horz_alignment = StyleTextAlignmentHorz::Center,
        }
    }

    if let Some(text_align) = rect.style.text_align {
        // Horizontal text alignment with higher priority
        horz_alignment = text_align;
    }

    (horz_alignment, vert_alignment)
}

/// Subtracts the padding from the bounds, returning the new bounds
///
/// Warning: The resulting rectangle may have negative width or height
fn subtract_padding(bounds: &TypedRect<f32, LayoutPixel>, padding: &LayoutPadding)
-> TypedRect<f32, LayoutPixel>
{
    let top     = padding.top.and_then(|top| Some(top.to_pixels())).unwrap_or(0.0);
    let bottom  = padding.bottom.and_then(|bottom| Some(bottom.to_pixels())).unwrap_or(0.0);
    let left    = padding.left.and_then(|left| Some(left.to_pixels())).unwrap_or(0.0);
    let right   = padding.right.and_then(|right| Some(right.to_pixels())).unwrap_or(0.0);

    let mut new_bounds = *bounds;

    new_bounds.origin.x += left;
    new_bounds.size.width -= right + left;
    new_bounds.origin.y += top;
    new_bounds.size.height -= top + bottom;

    new_bounds
}

/// Populate the style properties of the `DisplayRectangle`
fn populate_css_properties(
    rect: &mut DisplayRectangle,
    node_id: NodeId,
    css_overrides: &BTreeMap<NodeId, FastHashMap<String, CssProperty>>)
{
    use azul_css::CssProperty::{self, *};

    fn apply_style_property(rect: &mut DisplayRectangle, property: &CssProperty) {
        match property {
            BorderRadius(b)     => { rect.style.border_radius = Some(*b);                   },
            BackgroundColor(c)  => { rect.style.background_color = Some(*c);                },
            BackgroundSize(s)   => { rect.style.background_size = Some(*s);                 },
            BackgroundRepeat(r) => { rect.style.background_repeat = Some(*r);               },
            TextColor(t)        => { rect.style.font_color = Some(*t);                      },
            Border(b)           => { StyleBorder::merge(&mut rect.style.border, &b);        },
            Background(b)       => { rect.style.background = Some(b.clone());               },
            FontSize(f)         => { rect.style.font_size = Some(*f);                       },
            FontFamily(f)       => { rect.style.font_family = Some(f.clone());              },
            LetterSpacing(l)    => { rect.style.letter_spacing = Some(*l);                  },
            Overflow(o)         => { LayoutOverflow::merge(&mut rect.style.overflow, &o);   },
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

    use azul_css::DynamicCssPropertyDefault;

    // Assert that the types of two properties matches
    fn property_type_matches(a: &CssProperty, b: &DynamicCssPropertyDefault) -> bool {
        use std::mem::discriminant;
        use azul_css::DynamicCssPropertyDefault::*;
        match b {
            Exact(e) => discriminant(a) == discriminant(e),
            Auto => true, // "auto" always matches
        }
    }

    // Apply / static / dynamic properties
    for constraint in &rect.styled_node.css_constraints {
        use azul_css::CssDeclaration::*;
        match constraint {
            Static(static_property) => apply_style_property(rect, static_property),
            Dynamic(dynamic_property) => {
                if let Some(overridden_property) = css_overrides.get(&node_id).and_then(|overrides| overrides.get(&dynamic_property.dynamic_id)) {
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
