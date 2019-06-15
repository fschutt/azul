use std::{f32, collections::BTreeMap};
use azul_css::{
    RectLayout, StyleFontSize, RectStyle,
    StyleTextAlignmentHorz, StyleTextAlignmentVert,
    LayoutRect, LayoutSize,
};
use {
    id_tree::{NodeId, NodeDataContainer, NodeHierarchy},
    display_list::DisplayRectangle,
    dom::{NodeData, NodeType},
    app_resources::AppResources,
    text_layout::{Words, ScaledWords, WordPositions, LayoutedGlyphs},
};
use azul_core::{
    app_resources::{Au, FontInstanceKey},
    ui_solver::{PositionedRectangle, InlineTextLayout, LayoutResult, ResolvedTextLayoutOptions},
};
use azul_layout::{GetTextLayout, RectContent};

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
    use azul_core::ui_solver::DEFAULT_FONT_SIZE_PX;
    px_to_au(font_size.0.to_pixels(DEFAULT_FONT_SIZE_PX as f32))
}

pub(crate) fn px_to_au(px: f32) -> Au {
    use app_units::{Au as WrAu, AU_PER_PX, MIN_AU, MAX_AU};

    let target_app_units = WrAu((px * AU_PER_PX as f32) as i32);
    Au(target_app_units.min(MAX_AU).max(MIN_AU).0)
}

pub(crate) fn get_font_id(rect_style: &RectStyle) -> &str {
    use azul_core::ui_solver::DEFAULT_FONT_ID;
    let font_id = rect_style.font_family.as_ref().and_then(|family| family.get_property()?.fonts.get(0));
    font_id.map(|f| f.get_str()).unwrap_or(DEFAULT_FONT_ID)
}

pub(crate) fn get_font_size(rect_style: &RectStyle) -> StyleFontSize {
    use azul_core::ui_solver::DEFAULT_FONT_SIZE;
    rect_style.font_size.and_then(|fs| fs.get_property().cloned()).unwrap_or(DEFAULT_FONT_SIZE)
}

pub struct InlineText<'a> {
    words: &'a Words,
    scaled_words: &'a ScaledWords,
}

impl<'a> GetTextLayout for InlineText<'a> {
    fn get_text_layout(&mut self, text_layout_options: &ResolvedTextLayoutOptions) -> InlineTextLayout {
        use text_layout;
        let layouted_text_block = text_layout::position_words(
            self.words,
            self.scaled_words,
            text_layout_options,
        );
        // TODO: Cache the layouted text block on the &mut self
        text_layout::word_positions_to_inline_text_layout(&layouted_text_block, &self.scaled_words)
    }
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
    let mut solved_ui = {
        let rect_contents = create_rect_contents_cache(&word_cache, &scaled_words, node_data, app_resources);
        SolvedUi::new(bounding_rect, node_hierarchy, display_rects, rect_contents)
    };

    // TODO: overflowing rects!

    // Get the final word positions
    let positioned_word_cache = create_word_positions(&word_cache, &scaled_words, &solved_ui.solved_rects);
    let layouted_glyph_cache = get_glyphs(node_hierarchy, &scaled_words, &positioned_word_cache, &display_rects, &mut solved_ui.solved_rects);
    let node_depths = node_hierarchy.get_parents_sorted_by_depth();

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
    use azul_core::ui_solver::DEFAULT_FONT_SIZE_PX;

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
            font_size.0.to_pixels(DEFAULT_FONT_SIZE_PX as f32),
        );
        Some((*node_id, (scaled_words, *font_instance_key)))
    }).collect()
}

fn create_rect_contents_cache<'a, T>(
    words: &'a BTreeMap<NodeId, Words>,
    scaled_words: &'a BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
    display_rects: &NodeDataContainer<NodeData<T>>,
    app_resources: &AppResources,
) -> BTreeMap<NodeId, RectContent<InlineText<'a>>> {
    use azul_core::dom::NodeType::*;
    display_rects.linear_iter().filter_map(|node_id| {
        match *display_rects[node_id].get_node_type() {
            Image(id) => {
                let (w, h) = app_resources.get_image_info(&id)?.get_dimensions();
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

    use text_layout;
    words.iter().filter_map(|(node_id, words)| {
        let (scaled_words, font_instance_key) = scaled_words.get(&node_id)?;
        let (text_layout_options, _, _) = layouted_rects[*node_id].resolved_text_layout_options.as_ref()?;
        let positioned_words = text_layout::position_words(words, scaled_words, text_layout_options);
        Some((*node_id, (positioned_words, *font_instance_key)))
    }).collect()
}

fn get_glyphs<'a>(
    node_hierarchy: &NodeHierarchy,
    scaled_words: &BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
    positioned_word_cache: &BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    display_rects: &NodeDataContainer<DisplayRectangle<'a>>,
    positioned_rectangles: &mut NodeDataContainer<PositionedRectangle>,
) -> BTreeMap<NodeId, LayoutedGlyphs> {

    use text_layout::get_layouted_glyphs;

    scaled_words
    .iter()
    .filter_map(|(node_id, (scaled_words, _))| {

        let (word_positions, _) = positioned_word_cache.get(node_id)?;
        let display_rect = &display_rects[*node_id];
        let (horz_alignment, vert_alignment) = determine_text_alignment(&display_rect.style, &display_rect.layout);
        let parent_bounds = match &node_hierarchy[*node_id].parent {
            None => positioned_rectangles[NodeId::new(0)].bounds,
            Some(parent) => positioned_rectangles[*parent].bounds,
        };
        let bounds = positioned_rectangles[*node_id].bounds;
        let (_, inline_text_layout, _) = positioned_rectangles[*node_id].resolved_text_layout_options.as_mut()?;
        inline_text_layout.align_children_horizontal(horz_alignment);
        inline_text_layout.align_children_vertical_in_parent_bounds(&parent_bounds, vert_alignment);

        let glyphs = get_layouted_glyphs(word_positions, scaled_words, &inline_text_layout, bounds.origin);
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

#[cfg(test)]
mod layout_tests {

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

    /*#[test]
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
            width: Some(CssPropertyValue::Exact(LayoutWidth(PixelValue::px(500.0)))),
            min_width: None,
            max_width: None,
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(500.0));

        let layout = RectLayout {
            width: Some(CssPropertyValue::Exact(LayoutWidth(PixelValue::px(500.0)))),
            min_width: Some(CssPropertyValue::Exact(LayoutMinWidth(PixelValue::px(600.0)))),
            max_width: None,
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(600.0));

        let layout = RectLayout {
            width: Some(CssPropertyValue::Exact(LayoutWidth(PixelValue::px(10000.0)))),
            min_width: Some(CssPropertyValue::Exact(LayoutMinWidth(PixelValue::px(600.0)))),
            max_width: Some(CssPropertyValue::Exact(LayoutMaxWidth(PixelValue::px(800.0)))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(800.0));

        let layout = RectLayout {
            width: None,
            min_width: Some(CssPropertyValue::Exact(LayoutMinWidth(PixelValue::px(600.0)))),
            max_width: Some(CssPropertyValue::Exact(LayoutMaxWidth(PixelValue::px(800.0)))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::Between(600.0, 800.0));

        let layout = RectLayout {
            width: None,
            min_width: None,
            max_width: Some(CssPropertyValue::Exact(LayoutMaxWidth(PixelValue::px(800.0)))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::Between(0.0, 800.0));

        let layout = RectLayout {
            width: Some(CssPropertyValue::Exact(LayoutWidth(PixelValue::px(1000.0)))),
            min_width: None,
            max_width: Some(CssPropertyValue::Exact(LayoutMaxWidth(PixelValue::px(800.0)))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(800.0));

        let layout = RectLayout {
            width: Some(CssPropertyValue::Exact(LayoutWidth(PixelValue::px(1200.0)))),
            min_width: Some(CssPropertyValue::Exact(LayoutMinWidth(PixelValue::px(1000.0)))),
            max_width: Some(CssPropertyValue::Exact(LayoutMaxWidth(PixelValue::px(800.0)))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(800.0));

        let layout = RectLayout {
            width: Some(CssPropertyValue::Exact(LayoutWidth(PixelValue::px(1200.0)))),
            min_width: Some(CssPropertyValue::Exact(LayoutMinWidth(PixelValue::px(1000.0)))),
            max_width: Some(CssPropertyValue::Exact(LayoutMaxWidth(PixelValue::px(400.0)))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(400.0));
    }*/

    /*
    /// Tests that the nodes get filled correctly
    #[test]
    fn test_fill_out_preferred_width() {

        use azul_css::*;

        let (node_hierarchy, node_data) = get_display_rectangle_arena(&[
            (0, RectLayout {
                direction: Some(CssPropertyValue::Exact(LayoutDirection::Row)),
                .. Default::default()
            }),
            (1, RectLayout {
                max_width: Some(CssPropertyValue::Exact(LayoutMaxWidth(PixelValue::px(200.0)))),
                padding_left: Some(CssPropertyValue::Exact(LayoutPaddingLeft::px(20.0))),
                padding_right: Some(CssPropertyValue::Exact(LayoutPaddingRight::px(20.0))),
                direction: Some(CssPropertyValue::Exact(LayoutDirection::Row)),
                .. Default::default()
            }),
            (2, RectLayout {
                direction: Some(CssPropertyValue::Exact(LayoutDirection::Row)),
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
*/
}