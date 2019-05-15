use std::{f32, collections::BTreeMap};
use azul_css::{
    RectLayout, StyleFontSize, RectStyle,
    StyleTextAlignmentHorz, StyleTextAlignmentVert, PixelValue,
    LayoutRect, LayoutPoint, LayoutSize,
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
    ui_solver::PositionedRectangle,
};

const DEFAULT_FLEX_GROW_FACTOR: f32 = 1.0;
const DEFAULT_FONT_SIZE: StyleFontSize = StyleFontSize(PixelValue::const_px(10));
const DEFAULT_FONT_ID: &str = "sans-serif";

type PixelSize = f32;

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
    let font_id = rect_style.font_family.and_then(|family| family.get_property()?.fonts.get(0));
    font_id.map(|f| f.get_str()).unwrap_or(DEFAULT_FONT_ID)
}

pub(crate) fn get_font_size(rect_style: &RectStyle) -> StyleFontSize {
    rect_style.font_size.and_then(|fs| fs.get_property().cloned()).unwrap_or(DEFAULT_FONT_SIZE)
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
    bounding_rect: LayoutRect,
) -> LayoutResult {

    use azul_layout::SolvedUi;

    // 1. do layout pass without any text, only images, set display:inline children to (0px 0px)
    // 2. for each display:inline rect, layout children, calculate size of parent item
    // 3. for each rect, check if children overflow, if yes, reserve space for scrollbar
    // 4. copy UI and re-layout again, then copy result to all children of the overflowing rects
    // 5. return to caller, caller will do final text layout (not the job of the layout engine)

    let word_cache = create_word_cache(app_resources, node_data);
    let scaled_words = create_scaled_words(app_resources, &word_cache, display_rects);
    let rect_contents = create_rect_contents_cache();
    let ui = SolvedUi::new(bounding_rect, node_hierarchy, display_rects, rect_contents);
    let positioned_word_cache = create_word_positions(&word_cache, &scaled_words, display_rects, &proper_max_widths, &inline_text_blocks);
    let layouted_glyph_cache = get_glyphs(&scaled_words, &positioned_word_cache, &display_rects, &layouted_rects);

    // TODO: Set the final content sizes on layouted_rects!

    LayoutResult {
        rects: layouted_rects,
        word_cache,
        scaled_words,
        positioned_word_cache,
        layouted_glyph_cache,
        node_depths: solved_widths.non_leaf_nodes_sorted_by_depth,
    }
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
        line_height: rect.style.line_height.and_then(|lh| lh.get_property()).map(|lh| lh.0.get()),
        letter_spacing: rect.style.letter_spacing.and_then(|ls| ls.get_property()).map(|ls| ls.0.to_pixels()),
        word_spacing: rect.style.word_spacing.and_then(|ws| ws.get_property()).map(|ws| ws.0.to_pixels()),
        tab_width: rect.style.tab_width.and_then(|tw| tw.get_property()).map(|tw| tw.0.get()),
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

        let rect_padding_top = display_rect.layout.padding_top.and_then(|pt| pt.get_property_or_default()).unwrap_or_default().0.to_pixels();
        let rect_padding_left = display_rect.layout.padding_left.and_then(|pt| pt.get_property_or_default()).unwrap_or_default().0.to_pixels();
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

    if let Some(align_items) = rect_layout.align_items.and_then(|ai| ai.get_property_or_default()) {
        // Vertical text alignment
        use azul_css::LayoutAlignItems;
        match align_items {
            LayoutAlignItems::Start => vert_alignment = StyleTextAlignmentVert::Top,
            LayoutAlignItems::End => vert_alignment = StyleTextAlignmentVert::Bottom,
            // technically stretch = blocktext, but we don't have that yet
            _ => vert_alignment = StyleTextAlignmentVert::Center,
        }
    }

    if let Some(justify_content) = rect_layout.justify_content.and_then(|jc| jc.get_property_or_default()) {
        use azul_css::LayoutJustifyContent;
        // Horizontal text alignment
        match justify_content {
            LayoutJustifyContent::Start => horz_alignment = StyleTextAlignmentHorz::Left,
            LayoutJustifyContent::End => horz_alignment = StyleTextAlignmentHorz::Right,
            _ => horz_alignment = StyleTextAlignmentHorz::Center,
        }
    }

    if let Some(text_align) = rect_style.text_align.and_then(|ta| ta.get_property_or_default()) {
        // Horizontal text alignment with higher priority
        horz_alignment = text_align;
    }

    (horz_alignment, vert_alignment)
}

#[cfg(test)]
mod layout_tests {

    use azul_cssRectLayout;
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
