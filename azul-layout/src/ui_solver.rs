use std::{f32, collections::BTreeMap};
use crate::RectContent;
use azul_css::{
    RectLayout, RectStyle, StyleTextAlignmentHorz,
    StyleTextAlignmentVert, LayoutRect,
};
use azul_core::{
    id_tree::{NodeId, NodeDataContainer, NodeHierarchy},
    display_list::DisplayRectangle,
    dom::{NodeData, NodeType},
    app_resources::{AppResources, FontInstanceKey, Words, ScaledWords, WordPositions, LayoutedGlyphs},
    callbacks::PipelineId,
    ui_solver::{PositionedRectangle, LayoutResult},
};
use azul_text_layout::InlineText;

/// At this point in time, all font keys, image keys, etc. have
/// to be already submitted in the RenderApi!
pub fn do_the_layout(
    node_hierarchy: &NodeHierarchy,
    node_data: &NodeDataContainer<NodeData>,
    display_rects: &NodeDataContainer<DisplayRectangle>,
    app_resources: &AppResources,
    pipeline_id: &PipelineId,
    bounding_rect: LayoutRect,
) -> LayoutResult {

    use crate::SolvedUi;

    // 1. do layout pass without any text, only images, set display:inline children to (0px 0px)
    // 2. for each display:inline rect, layout children, calculate size of parent item
    // 3. for each rect, check if children overflow, if yes, reserve space for scrollbar
    // 4. copy UI and re-layout again, then copy result to all children of the overflowing rects
    // 5. return to caller, caller will do final text layout (not the job of the layout engine)

    let node_depths = node_hierarchy.get_parents_sorted_by_depth();
    let word_cache = create_word_cache(app_resources, node_data);
    let scaled_words = create_scaled_words(app_resources, pipeline_id, &word_cache, display_rects);
    let mut solved_ui = {
        let mut rect_contents = create_rect_contents_cache(app_resources, pipeline_id, &word_cache, &scaled_words, node_data);
        SolvedUi::new(bounding_rect, node_hierarchy, display_rects, &mut rect_contents, &node_depths)
    };

    // TODO: overflowing rects!

    // Get the final word positions
    let positioned_word_cache = create_word_positions(&word_cache, &scaled_words, &solved_ui.solved_rects);
    let layouted_glyph_cache = get_glyphs(node_hierarchy, &scaled_words, &positioned_word_cache, &display_rects, &mut solved_ui.solved_rects);

    // TODO: Set the final content sizes on layouted_rects!

    LayoutResult {
        rects: solved_ui.solved_rects,
        word_cache,
        scaled_words,
        positioned_word_cache,
        layouted_glyph_cache,
        node_depths,
    }
}

pub fn create_word_cache(
    app_resources: &AppResources,
    node_data: &NodeDataContainer<NodeData>,
) -> BTreeMap<NodeId, Words> {
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
    app_resources: &AppResources,
    pipeline_id: &PipelineId,
    words: &BTreeMap<NodeId, Words>,
    display_rects: &NodeDataContainer<DisplayRectangle>,
) -> BTreeMap<NodeId, (ScaledWords, FontInstanceKey)> {

    use azul_core::{
        app_resources::{ImmediateFontId, font_size_to_au, get_font_id, get_font_size},
        ui_solver::DEFAULT_FONT_SIZE_PX,
    };
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

fn create_rect_contents_cache<'a>(
    app_resources: &AppResources,
    pipeline_id: &PipelineId,
    words: &'a BTreeMap<NodeId, Words>,
    scaled_words: &'a BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
    display_rects: &NodeDataContainer<NodeData>,
) -> BTreeMap<NodeId, RectContent<InlineText<'a>>> {
    use azul_core::dom::NodeType::*;
    display_rects.linear_iter().filter_map(|node_id| {
        match *display_rects[node_id].get_node_type() {
            Image(id) => {
                let (w, h) = app_resources.get_image_info(pipeline_id, &id)?.get_dimensions();
                Some((node_id, RectContent::Image(w, h)))
            },
            Text(_) | Label(_) => {
                Some((node_id, RectContent::Text(InlineText {
                    words: words.get(&node_id)?,
                    scaled_words: scaled_words.get(&node_id).map(|(sw, _)| sw)?,
                })))
            },
            _ => None,
        }
    }).collect()
}

fn create_word_positions<'a>(
     words: &BTreeMap<NodeId, Words>,
     scaled_words: &BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
     layouted_rects: &NodeDataContainer<PositionedRectangle>,
) -> BTreeMap<NodeId, (WordPositions, FontInstanceKey)> {
    use azul_text_layout::text_layout;
    words.iter().filter_map(|(node_id, words)| {
        let (scaled_words, font_instance_key) = scaled_words.get(&node_id)?;
        let (text_layout_options, _, _) = layouted_rects[*node_id].resolved_text_layout_options.as_ref()?;
        let positioned_words = text_layout::position_words(words, scaled_words, text_layout_options);
        Some((*node_id, (positioned_words, *font_instance_key)))
    }).collect()
}

fn get_glyphs(
    node_hierarchy: &NodeHierarchy,
    scaled_words: &BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
    positioned_word_cache: &BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    display_rects: &NodeDataContainer<DisplayRectangle>,
    positioned_rectangles: &mut NodeDataContainer<PositionedRectangle>,
) -> BTreeMap<NodeId, LayoutedGlyphs> {

    use azul_text_layout::text_layout::get_layouted_glyphs;

    scaled_words
    .iter()
    .filter_map(|(node_id, (scaled_words, _))| {

        let (word_positions, _) = positioned_word_cache.get(node_id)?;
        let display_rect = &display_rects[*node_id];
        let (horz_alignment, vert_alignment) = determine_text_alignment(&display_rect.style, &display_rect.layout);
        let parent_node_id = node_hierarchy[*node_id].parent.unwrap_or(NodeId::new(0));
        let parent_size = positioned_rectangles[parent_node_id].size;

        let (_, inline_text_layout, _) = positioned_rectangles[*node_id].resolved_text_layout_options.as_mut()?;
        inline_text_layout.align_children_horizontal(horz_alignment);
        inline_text_layout.align_children_vertical_in_parent_bounds(&parent_size, vert_alignment);

        let glyphs = get_layouted_glyphs(word_positions, scaled_words, &inline_text_layout);
        Some((*node_id, glyphs))
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

