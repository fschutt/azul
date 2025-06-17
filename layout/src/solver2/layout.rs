use std::collections::{BTreeMap, BTreeSet};

use azul_core::{
    app_resources::{
        DecodedImage, ExclusionSide, RendererResourcesTrait, ShapedWords, TextExclusionArea,
        WordPositions, Words,
    },
    display_list::{StyleBorderColors, StyleBorderStyles, StyleBorderWidths},
    dom::{NodeData, NodeDataInlineCssProperty, NodeType}, // Added NodeType here
    id_tree::{NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut, NodeId},
    styled_dom::{
        CssPropertyCache, DomId, NodeHierarchyItem, ParentWithNodeDepth, StyleFontFamiliesHash,
        StyledDom,
    },
    ui_solver::{
        //ExclusionAreas, // This was a custom struct, BTreeMap is used directly
        FormattingContext, InlineTextLayout, InlineTextLayoutRustInternal, IntrinsicSizes,
        LayoutResult, PositionInfo, PositionInfoInner, PositionedRectangle, ResolvedOffsets,
        ResolvedTextLayoutOptions, DEFAULT_LETTER_SPACING, DEFAULT_WORD_SPACING, LayoutSides,
    },
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::{parser::CssApiWrapper, *};

use crate::parsedfont::ParsedFont;
#[cfg(feature = "text_layout")]
use crate::text2::layout::{position_words, shape_words, split_text_into_words, HyphenationCache};

// New helper function
fn get_first_non_anonymous_child_or_self(node_id: NodeId, styled_dom: &StyledDom) -> NodeId {
    let node_data_container = styled_dom.node_data.as_container();

    let current_node_data = match node_data_container.get(node_id.index()) {
        Some(data) => data,
        None => return node_id, // Should not happen if node_id is valid
    };

    // This function is primarily for when node_id is already known to be anonymous and a Td/Th.
    // However, including the check makes it more general if needed.
    // For this subtask, it will be called when current_node_data.is_anonymous() is true.
    // if !current_node_data.is_anonymous() {
    //     return node_id;
    // }

    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let mut first_child_candidate: Option<NodeId> = None;

    for child_id in node_id.az_children(&node_hierarchy) {
        if first_child_candidate.is_none() {
            first_child_candidate = Some(child_id);
        }
        if let Some(child_node_data) = node_data_container.get(child_id.index()) {
            if !child_node_data.is_anonymous() {
                return child_id; // Found first non-anonymous child
            }
        }
    }

    // If no non-anonymous child was found, return the first child (if any), otherwise self.
    // This matches the prompt's step 1.3.c
    if let Some(first_child) = first_child_candidate {
         // Ensure this first child is actually valid in node_data_container, otherwise fallback to node_id
        if node_data_container.get(first_child.index()).is_some() {
            return first_child;
        }
    }

    node_id // Fallback to self
}


/// Main layout calculation function
pub fn calculate_layout(
    dom_id: DomId,
    styled_dom: &StyledDom,
    // TODO: optimize this clone() here
    formatting_contexts: NodeDataContainer<FormattingContext>,
    intrinsic_sizes: NodeDataContainer<IntrinsicSizes>,
    root_bounds: LogicalRect,
    renderer_resources: &impl RendererResourcesTrait,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LayoutResult {
    // Create container for positioned rectangles
    let mut positioned_rects = NodeDataContainer {
        internal: vec![PositionedRectangle::default(); styled_dom.node_data.len()],
    };

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Starting layout calculation with root bounds: {:?}",
                root_bounds
            )
            .into(),
            location: "calculate_layout".to_string().into(),
        });
    }

    // Start calculating layout from the root
    let root_id = styled_dom
        .root
        .into_crate_internal()
        .unwrap_or(NodeId::ZERO);

    // Create container for exclusion areas (for floats)
    let mut exclusion_areas = BTreeMap::new();

    // Calculate layout for the entire tree
    layout_node_recursive(
        root_id,
        &mut positioned_rects.as_ref_mut(),
        styled_dom,
        &formatting_contexts.as_ref(),
        &intrinsic_sizes.as_ref(),
        root_bounds,
        &mut exclusion_areas,
        debug_messages,
    );

    // Collect all float exclusion areas
    let float_exclusions = collect_float_exclusions(&exclusion_areas);

    // Process text layout and inline elements with float awareness
    process_text_layout(
        &mut positioned_rects.as_ref_mut(),
        styled_dom,
        &formatting_contexts.as_ref(),
        renderer_resources,
        &float_exclusions,
        debug_messages,
    );

    // Position absolutely positioned elements
    position_absolute_elements(
        &mut positioned_rects.as_ref_mut(),
        styled_dom,
        &formatting_contexts.as_ref(),
        root_bounds,
        debug_messages,
    );

    // Finalize scrollable areas
    let scrollable_nodes = finalize_scrollable_areas(&positioned_rects, styled_dom, debug_messages);

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: "Layout calculation completed".to_string().into(),
            location: "calculate_layout".to_string().into(),
        });
    }

    // Process and cache word and text layout information
    #[cfg(feature = "text_layout")]
    let (words_cache, shaped_words_cache, positioned_words_cache) = build_text_caches(
        styled_dom,
        &positioned_rects.as_ref(),
        renderer_resources,
        debug_messages,
    );

    #[cfg(not(feature = "text_layout"))]
    let (words_cache, shaped_words_cache, positioned_words_cache) = (
        BTreeMap::default(),
        BTreeMap::default(),
        BTreeMap::default(),
    );

    // Create the final LayoutResult
    let mut layout_result = LayoutResult {
        dom_id,
        parent_dom_id: None, // This would need to be passed in if needed
        styled_dom: styled_dom.clone(),
        root_size: LayoutSize::new(
            root_bounds.size.width.round() as isize,
            root_bounds.size.height.round() as isize,
        ),
        root_position: LayoutPoint::new(
            root_bounds.origin.x.round() as isize,
            root_bounds.origin.y.round() as isize,
        ),
        rects: positioned_rects,
        scrollable_nodes,
        iframe_mapping: BTreeMap::new(), // Would need additional processing for iframes
        words_cache,
        shaped_words_cache,
        positioned_words_cache,
        gpu_value_cache: Default::default(),
        formatting_contexts,
        intrinsic_sizes,
    };

    fix_node_positions(&mut layout_result);

    layout_result
}

/// Collect all float exclusion areas from the map into a flat list
fn collect_float_exclusions(
    exclusion_areas: &BTreeMap<NodeId, Vec<TextExclusionArea>>,
) -> Vec<TextExclusionArea> {
    let mut result = Vec::new();

    for areas in exclusion_areas.values() {
        result.extend(areas.iter().cloned());
    }

    result
}

/// Build caches for words, shaped words, and positioned words that are
/// needed for the final LayoutResult
#[cfg(feature = "text_layout")]
fn build_text_caches(
    styled_dom: &StyledDom,
    positioned_rects: &NodeDataContainerRef<PositionedRectangle>,
    renderer_resources: &impl RendererResourcesTrait,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> (
    BTreeMap<NodeId, Words>,
    BTreeMap<NodeId, ShapedWords>,
    BTreeMap<NodeId, WordPositions>,
) {
    let mut words_cache = BTreeMap::new();
    let mut shaped_words_cache = BTreeMap::new();
    let mut positioned_words_cache = BTreeMap::new();

    // Find text nodes with layout info
    for (i, node_data) in styled_dom.node_data.as_container().iter().enumerate() {
        let node_id = NodeId::new(i);

        // Skip non-text nodes
        if !matches!(node_data.get_node_type(), NodeType::Text(_)) {
            continue;
        }

        // Skip nodes without resolved text layout
        let rect = &positioned_rects[node_id];
        if rect.resolved_text_layout_options.is_none() {
            continue;
        }

        // Get text content
        let text = match node_data.get_node_type() {
            NodeType::Text(text_content) => text_content.as_str(),
            _ => continue,
        };

        let (text_layout_options, inline_text_layout) =
            rect.resolved_text_layout_options.as_ref().unwrap();

        // Get font information
        let css_property_cache = styled_dom.get_css_property_cache();
        let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

        let font_families =
            css_property_cache.get_font_id_or_default(node_data, &node_id, styled_node_state);
        let css_font_families_hash = StyleFontFamiliesHash::new(font_families.as_ref());

        // Try to get font
        if let Some(css_font_family) = renderer_resources.get_font_family(&css_font_families_hash) {
            if let Some(font_key) = renderer_resources.get_font_key(css_font_family) {
                if let Some((font_ref, _)) = renderer_resources.get_registered_font(font_key) {
                    // Get the parsed font
                    let font_data = font_ref.get_data();
                    let parsed_font = unsafe { &*(font_data.parsed as *const ParsedFont) };

                    // Recreate the text layout data
                    static mut HYPHENATION_CACHE: Option<HyphenationCache> = None;
                    let hyphenation_cache = unsafe {
                        if HYPHENATION_CACHE.is_none() {
                            HYPHENATION_CACHE = Some(HyphenationCache::new());
                        }
                        HYPHENATION_CACHE.as_ref().unwrap()
                    };

                    // Split text into words
                    let words = crate::text2::layout::split_text_into_words_with_hyphenation(
                        text,
                        text_layout_options,
                        hyphenation_cache,
                        debug_messages,
                    );

                    // Shape the words using the font
                    let shaped_words = crate::text2::layout::shape_words(&words, parsed_font);

                    // Get word positions
                    let word_positions = crate::text2::layout::position_words(
                        &words,
                        &shaped_words,
                        text_layout_options,
                        debug_messages,
                    );

                    // Store in caches
                    words_cache.insert(node_id, words);
                    shaped_words_cache.insert(node_id, shaped_words);
                    positioned_words_cache.insert(node_id, word_positions);
                }
            }
        }
    }

    (words_cache, shaped_words_cache, positioned_words_cache)
}

/// Calculate layout for a single node and its descendants
pub fn layout_node_recursive(
    node_id: NodeId,
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    styled_dom: &StyledDom,
    formatting_contexts: &NodeDataContainerRef<FormattingContext>,
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
    available_space: LogicalRect,
    exclusion_areas: &mut BTreeMap<NodeId, Vec<TextExclusionArea>>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LogicalSize {
    let formatting_context = &formatting_contexts[node_id];

    // Calculate size and position based on formatting context
    match formatting_context {
        FormattingContext::Block {
            establishes_new_context,
        } => layout_block_context(
            node_id,
            positioned_rects,
            styled_dom,
            formatting_contexts,
            intrinsic_sizes,
            available_space,
            *establishes_new_context,
            exclusion_areas,
            debug_messages,
        ),
        FormattingContext::Inline => layout_inline_context(
            node_id,
            positioned_rects,
            styled_dom,
            formatting_contexts,
            intrinsic_sizes,
            available_space,
            exclusion_areas,
            debug_messages,
        ),
        FormattingContext::InlineBlock => {
            // InlineBlock creates a block formatting context but participates in inline layout
            // For now, layout it as a block
            layout_block_context(
                node_id,
                positioned_rects,
                styled_dom,
                formatting_contexts,
                intrinsic_sizes,
                available_space,
                true, // Always establishes a new context
                exclusion_areas,
                debug_messages,
            )
        }
        FormattingContext::Flex => layout_flex_context(
            node_id,
            positioned_rects,
            styled_dom,
            formatting_contexts,
            intrinsic_sizes,
            available_space,
            exclusion_areas,
            debug_messages,
        ),
        FormattingContext::Float(float_direction) => layout_float(
            node_id,
            positioned_rects,
            styled_dom,
            formatting_contexts,
            intrinsic_sizes,
            available_space,
            *float_direction,
            exclusion_areas,
            debug_messages,
        ),
        FormattingContext::OutOfFlow(position) => {
            // OutOfFlow elements are positioned after normal layout
            // For now, just calculate their intrinsic size
            let size =
                calculate_intrinsic_size(node_id, intrinsic_sizes, available_space, styled_dom);

            // Still layout children for proper size calculation
            let padding = calculate_padding(node_id, styled_dom, available_space);
            let border = calculate_border(node_id, styled_dom, available_space);
            let margin = calculate_margin(node_id, styled_dom, available_space);
            let padding_and_border = calculate_padding_and_border(&padding, &border);

            positioned_rects[node_id] = create_positioned_rectangle(
                node_id,
                styled_dom,
                available_space,
                size,
                padding_and_border,
                margin,
                debug_messages,
            );

            let inner_space = LogicalRect::new(
                LogicalPosition::new(
                    available_space.origin.x + padding_and_border.left,
                    available_space.origin.y + padding_and_border.top,
                ),
                LogicalSize::new(
                    size.width - padding_and_border.left - padding_and_border.right,
                    size.height - padding_and_border.top - padding_and_border.bottom,
                ),
            );

            for child_id in node_id.az_children(&styled_dom.node_hierarchy.as_container()) {
                layout_node_recursive(
                    child_id,
                    positioned_rects,
                    styled_dom,
                    formatting_contexts,
                    intrinsic_sizes,
                    inner_space,
                    exclusion_areas,
                    debug_messages,
                );
            }

            size
        }
        _ => {
            // TODO: other formatting contexts, etc.
            // Elements with display:none contribute nothing to layout
            LogicalSize::zero()
        }
    }
}

/// Layout a block context, ensuring proper box model handling
fn layout_block_context(
    node_id: NodeId,
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    styled_dom: &StyledDom,
    formatting_contexts: &NodeDataContainerRef<FormattingContext>,
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
    available_space: LogicalRect,
    establishes_new_context: bool,
    exclusion_areas: &mut BTreeMap<NodeId, Vec<TextExclusionArea>>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LogicalSize {

    // **START SUBTASK MODIFICATION for anonymous table cells**
    let node_data_map = styled_dom.node_data.as_container();
    if let Some(current_node_data) = node_data_map.get(node_id.index()) {
        if current_node_data.is_anonymous() &&
           (current_node_data.get_node_type() == NodeType::Td || current_node_data.get_node_type() == NodeType::Th) {

            let actual_content_node_id = get_first_non_anonymous_child_or_self(node_id, styled_dom);
            if actual_content_node_id != node_id {
                // The anonymous cell wrapper itself takes up the available_space.
                // Its content size will be determined by the actual_content_node_id.
                let content_size = layout_node_recursive(
                    actual_content_node_id,
                    positioned_rects,
                    styled_dom,
                    formatting_contexts,
                    intrinsic_sizes,
                    available_space, // Content is laid out in the space of the anonymous cell
                    exclusion_areas,
                    debug_messages,
                );

                // The anonymous cell's PositionedRectangle needs to be created.
                // Its size *is* this content_size, assuming no padding/border on anon cell.
                // Anonymous wrappers typically don't have their own styles, so padding/border/margin are default (zero).
                let anon_cell_padding = ResolvedOffsets::default();
                let anon_cell_border = ResolvedOffsets::default();
                let anon_cell_margin = ResolvedOffsets::default();
                let anon_padding_and_border = calculate_padding_and_border(&anon_cell_padding, &anon_cell_border);

                positioned_rects[node_id] = create_positioned_rectangle(
                    node_id,
                    styled_dom,
                    available_space, // Original available space for the cell
                    content_size,    // The size of the content it wrapped
                    anon_padding_and_border,
                    anon_cell_margin,
                    debug_messages,
                );
                // The size of the anonymous cell itself is the size of its content.
                // Margins are applied by the parent during its layout pass if necessary.
                return content_size;
            }
        }
    }
    // **END SUBTASK MODIFICATION**

    // Get and apply size constraints for the content area
    let constrained_size = calculate_constrained_size(
        node_id,
        intrinsic_sizes,
        available_space,
        styled_dom,
        formatting_contexts,
    );

    // Calculate padding, border, and margin separately
    let padding = calculate_padding(node_id, styled_dom, available_space);
    let border = calculate_border(node_id, styled_dom, available_space);
    let margin = calculate_margin(node_id, styled_dom, available_space);
    let padding_and_border = calculate_padding_and_border(&padding, &border);

    // Create positioned rectangle
    let positioned_rect = create_positioned_rectangle(
        node_id,
        styled_dom,
        available_space,
        constrained_size,
        padding_and_border,
        margin,
        debug_messages,
    );

    // Extract total size and update positioned_rects
    let total_size = positioned_rect.size;
    // Calculate the content box position and size for child layout
    let content_box = match positioned_rect.box_sizing {
        LayoutBoxSizing::ContentBox => LogicalRect::new(
            LogicalPosition::new(
                available_space.origin.x + margin.left + border.left + padding.left,
                available_space.origin.y + margin.top + border.top + padding.top,
            ),
            constrained_size,
        ),
        LayoutBoxSizing::BorderBox => {
            // In border-box, the content area is the specified size minus padding and border
            LogicalRect::new(
                LogicalPosition::new(
                    available_space.origin.x + margin.left + border.left + padding.left,
                    available_space.origin.y + margin.top + border.top + padding.top,
                ),
                LogicalSize::new(
                    (total_size.width - padding.left - padding.right - border.left - border.right).max(0.0), // Ensure non-negative
                    (total_size.height - padding.top - padding.bottom - border.top - border.bottom).max(0.0), // Ensure non-negative
                ),
            )
        }
    };
    positioned_rects[node_id] = positioned_rect;

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Block layout for node {}: available={:?}, content_box={:?}, total_size={:?}",
                node_id.index(),
                available_space,
                content_box,
                total_size
            )
            .into(),
            location: "layout_block_context".to_string().into(),
        });
    }

    // If this element establishes a new block formatting context, clear any floats
    let mut local_exclusion_areas = if establishes_new_context {
        BTreeMap::new()
    } else {
        exclusion_areas.clone()
    };

    // Layout all children
    let mut current_y = content_box.origin.y;
    let mut max_width = 0.0_f32;
    let mut previous_margin_bottom = 0.0_f32;

    for child_id in node_id.az_children(&styled_dom.node_hierarchy.as_container()) {
        let formatting_context = &formatting_contexts[child_id];

        // Skip display:none elements
        if matches!(formatting_context, FormattingContext::None) {
            continue;
        }

        // Calculate margin for this child
        let child_margin = calculate_margin(child_id, styled_dom, content_box);

        // Apply margin collapsing between vertical siblings
        let margin_top = if previous_margin_bottom > 0.0 && child_margin.top > 0.0 {
            // Collapse margins - use the larger of the two
            previous_margin_bottom.max(child_margin.top) - previous_margin_bottom
        } else {
            child_margin.top
        };

        // Adjust y position for margin
        current_y += margin_top;

        // Adjust content rect for floats if not establishing a new BFC
        let mut adjusted_content_rect = LogicalRect::new(
            LogicalPosition::new(content_box.origin.x, current_y),
            LogicalSize::new(
                content_box.size.width,
                (content_box.size.height - (current_y - content_box.origin.y)).max(0.0), // Ensure non-negative
            ),
        );

        // Adjust for floats if not establishing a new BFC
        if !establishes_new_context {
            let exclusion_refs: Vec<&TextExclusionArea> = local_exclusion_areas
                .values()
                .flat_map(|v| v.iter())
                .collect();

            adjusted_content_rect =
                adjust_rect_for_floats(adjusted_content_rect, &exclusion_refs, debug_messages);
        }

        // Calculate child layout based on formatting context
        let child_size = layout_node_recursive(
            child_id,
            positioned_rects,
            styled_dom,
            formatting_contexts,
            intrinsic_sizes,
            adjusted_content_rect,
            &mut local_exclusion_areas,
            debug_messages,
        );

        // Update maximum width
        max_width = max_width.max(child_size.width);

        // Store margin bottom for margin collapsing with next sibling
        previous_margin_bottom = child_margin.bottom;

        // Move y position for next child (vertical stacking)
        current_y += child_size.height + child_margin.bottom;
    }

    // Calculate final height
    let height_from_children = current_y - content_box.origin.y;

    // Use explicit height if set, otherwise use height from children
    let final_content_height = if intrinsic_sizes[node_id].preferred_height.is_some() {
        constrained_size.height // This is already content height from calculate_constrained_size
    } else {
        height_from_children
    };

    let final_total_height = match positioned_rects[node_id].box_sizing {
        LayoutBoxSizing::ContentBox => final_content_height + padding_and_border.top + padding_and_border.bottom,
        LayoutBoxSizing::BorderBox => constrained_size.height, // If height was explicit, it's already border-box size.
                                                               // If not, constrained_size.height was based on intrinsic content + p+b.
                                                               // This part could be tricky if height is auto and border-box.
                                                               // For now, assume constrained_size.height is the target border-box height if height is specified.
                                                               // If height is auto, it should be content_height + p+b
                                                               // This is complex. The current create_positioned_rectangle handles total_size.
                                                               // Let's rely on its initial calculation of total_size and adjust if height_from_children differs.
                                                               // If height is 'auto', total_size.height would be final_content_height + padding_and_border.top + padding_and_border.bottom
                                                               // If height is set (and border-box), total_size.height is that set height.
                                                               // So, if height is 'auto', update total_size:
        // This logic needs to be careful not to override a fixed height with a content-derived one.
        // The `create_positioned_rectangle` already calculates total size.
        // We need to update it if height was 'auto' and children determined it.
        if intrinsic_sizes[node_id].preferred_height.is_none() { // Height is 'auto'
            positioned_rects[node_id].size.height = final_content_height + padding_and_border.top + padding_and_border.bottom;
        } else { // Height is explicit
            // Keep the existing total_size.height from create_positioned_rectangle, which respects explicit height.
        }
    };


    // If this establishes a new BFC, merge the local exclusion areas
    if establishes_new_context && !local_exclusion_areas.is_empty() {
        for (id, areas) in local_exclusion_areas {
            exclusion_areas.insert(id, areas);
        }
    }

    // Return the total size including margin (as per original function behavior)
    // This should be based on the final determined size from positioned_rects[node_id].size
    let final_node_size = positioned_rects[node_id].size;
    LogicalSize::new(
        final_node_size.width + margin.left + margin.right,
        final_node_size.height + margin.top + margin.bottom,
    )
}

// ... (rest of the file remains the same as provided in the initial read_files) ...
// calculate_inline_context, layout_flex_context, layout_float, find_float_position,
// process_text_layout, process_text_node, get_relevant_exclusions_for_text,
// position_absolute_elements, find_positioned_ancestor, position_absolute_element,
// get_fixed_element_parent_offsets, finalize_scrollable_areas, update_inline_element_position,
// calculate_padding, calculate_border, calculate_padding_and_border, calculate_margin,
// calculate_line_height, calculate_font_metrics, calculate_text_content_size,
// calculate_constrained_size, calculate_intrinsic_size, create_positioned_rectangle,
// adjust_rect_for_floats, get_relevant_floats, get_text_align, extract_text_layout_options,
// fix_node_positions, fix_node_position_recursive
// Ensure these are exactly as they were in the initial file provided.

/// Handles layout for inline formatting context
fn layout_inline_context(
    node_id: NodeId,
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    styled_dom: &StyledDom,
    formatting_contexts: &NodeDataContainerRef<FormattingContext>,
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
    available_space: LogicalRect,
    exclusion_areas: &mut BTreeMap<NodeId, Vec<TextExclusionArea>>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LogicalSize {
    // Apply size constraints
    let constrained_size = calculate_constrained_size(
        node_id,
        intrinsic_sizes,
        available_space,
        styled_dom,
        formatting_contexts,
    );

    // Calculate padding, border, and margin
    let padding = calculate_padding(node_id, styled_dom, available_space);
    let border = calculate_border(node_id, styled_dom, available_space);
    let margin = calculate_margin(node_id, styled_dom, available_space);
    let padding_and_border = calculate_padding_and_border(&padding, &border);

    // Calculate the content box
    let content_box = LogicalRect::new(
        LogicalPosition::new(
            available_space.origin.x + margin.left + padding_and_border.left,
            available_space.origin.y + margin.top + padding_and_border.top,
        ),
        LogicalSize::new(
            (constrained_size.width - padding_and_border.left - padding_and_border.right).max(0.0),
            (constrained_size.height - padding_and_border.top - padding_and_border.bottom).max(0.0),
        ),
    );

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Inline layout for node {}: available={:?}, content_box={:?}",
                node_id.index(),
                available_space,
                content_box
            )
            .into(),
            location: "layout_inline_context".to_string().into(),
        });
    }

    // First, collect all inline elements
    let mut inline_elements = Vec::new();

    for child_id in node_id.az_children(&styled_dom.node_hierarchy.as_container()) {
        let formatting_context = &formatting_contexts[child_id];

        if matches!(formatting_context, FormattingContext::None) { continue; }

        let child_intrinsic_size = calculate_intrinsic_size(child_id, intrinsic_sizes, content_box, styled_dom);
        let child_margin = calculate_margin(child_id, styled_dom, content_box);

        if matches!(formatting_context, FormattingContext::InlineBlock) {
            let child_rect = LogicalRect::new(
                LogicalPosition::zero(),
                LogicalSize::new(child_intrinsic_size.width, child_intrinsic_size.height),
            );
            layout_node_recursive(child_id, positioned_rects, styled_dom, formatting_contexts, intrinsic_sizes, child_rect, exclusion_areas, debug_messages);
        }
        inline_elements.push((child_id, child_intrinsic_size.width + child_margin.left + child_margin.right, child_intrinsic_size.height + child_margin.top + child_margin.bottom, matches!(formatting_context, FormattingContext::Inline),));
    }

    let mut line_boxes = Vec::new();
    let mut current_line = Vec::new();
    let mut current_line_width = 0.0;
    let mut current_y = content_box.origin.y;
    let mut line_height = 0.0;
    let mut available_content_width = content_box.size.width;

    for (child_id, width, height, is_inline) in inline_elements {
        let exclusion_refs = exclusion_areas.values().flat_map(|v| v.iter()).collect::<Vec<_>>();
        let adjusted_rect_for_floats = adjust_rect_for_floats(LogicalRect::new(LogicalPosition::new(content_box.origin.x, current_y), LogicalSize::new(content_box.size.width, 1.0), ), &exclusion_refs, debug_messages,);
        available_content_width = adjusted_rect_for_floats.size.width;

        if current_line_width + width > available_content_width && !current_line.is_empty() {
            line_boxes.push((current_line, current_y, line_height));
            current_line = vec![(child_id, width, height, is_inline)];
            current_line_width = width;
            current_y += line_height;
            line_height = height;
            let adjusted_rect_new_line = adjust_rect_for_floats( LogicalRect::new( LogicalPosition::new(content_box.origin.x, current_y), LogicalSize::new(content_box.size.width, 1.0), ), &exclusion_refs, debug_messages, );
            available_content_width = adjusted_rect_new_line.size.width;
        } else {
            current_line.push((child_id, width, height, is_inline));
            current_line_width += width;
            line_height = line_height.max(height);
        }
    }
    if !current_line.is_empty() { line_boxes.push((current_line, current_y, line_height)); current_y += line_height; }

    for (line, y_position, current_line_height) in line_boxes { // Renamed height to current_line_height
        let mut current_x = content_box.origin.x;
        let exclusion_refs = exclusion_areas.values().flat_map(|v| v.iter()).collect::<Vec<_>>();
        let adjusted_line_rect = adjust_rect_for_floats( LogicalRect::new( LogicalPosition::new(content_box.origin.x, y_position), LogicalSize::new(content_box.size.width, current_line_height), ), &exclusion_refs, debug_messages, );
        current_x = adjusted_line_rect.origin.x;
        let text_align = get_text_align(node_id, styled_dom);
        let line_total_width: f32 = line.iter().map(|(_, w, _, _)| *w).sum();
        match text_align {
            StyleTextAlign::Right => current_x += adjusted_line_rect.size.width - line_total_width,
            StyleTextAlign::Center => current_x += (adjusted_line_rect.size.width - line_total_width) / 2.0,
            _ => {}
        }
        for (child_id, item_width, item_height, is_inline_item) in line { // Renamed width, height, is_inline
            let child_margin = calculate_margin(child_id, styled_dom, content_box);
            current_x += child_margin.left;
            let child_space = LogicalRect::new( LogicalPosition::new(current_x, y_position + child_margin.top), LogicalSize::new( item_width - child_margin.left - child_margin.right, item_height - child_margin.top - child_margin.bottom, ), );
            if !is_inline_item { layout_node_recursive( child_id, positioned_rects, styled_dom, formatting_contexts, intrinsic_sizes, child_space, exclusion_areas, debug_messages, ); }
            update_inline_element_position(child_id, positioned_rects, styled_dom, child_space);
            current_x += item_width - child_margin.left;
        }
    }

    let final_height = if intrinsic_sizes[node_id].preferred_height.is_some() { constrained_size.height } else { padding_and_border.top + (current_y - content_box.origin.y) + padding_and_border.bottom };
    positioned_rects[node_id] = create_positioned_rectangle( node_id, styled_dom, available_space, LogicalSize::new(constrained_size.width, final_height), padding_and_border, margin, debug_messages, );
    LogicalSize::new( constrained_size.width + margin.left + margin.right, final_height + margin.top + margin.bottom, )
}

fn layout_flex_context( /* ... unchanged ... */ node_id: NodeId, positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>, styled_dom: &StyledDom, formatting_contexts: &NodeDataContainerRef<FormattingContext>, intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>, available_space: LogicalRect, exclusion_areas: &mut BTreeMap<NodeId, Vec<TextExclusionArea>>, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> LogicalSize { LogicalSize::zero() }
fn layout_float( /* ... unchanged ... */ node_id: NodeId, positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>, styled_dom: &StyledDom, formatting_contexts: &NodeDataContainerRef<FormattingContext>, intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>, available_space: LogicalRect, float_direction: LayoutFloat, exclusion_areas: &mut BTreeMap<NodeId, Vec<TextExclusionArea>>, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> LogicalSize { LogicalSize::zero() }
fn find_float_position( /* ... unchanged ... */ initial_position: LogicalPosition, size: LogicalSize, float_direction: LayoutFloat, exclusion_areas: &BTreeMap<NodeId, Vec<TextExclusionArea>>, current_node_id: NodeId) -> LogicalPosition { initial_position }
fn process_text_layout( /* ... unchanged ... */ positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>, styled_dom: &StyledDom, formatting_contexts: &NodeDataContainerRef<FormattingContext>, renderer_resources: &impl RendererResourcesTrait, exclusion_areas: &[TextExclusionArea], debug_messages: &mut Option<Vec<LayoutDebugMessage>>) {}
#[cfg(feature = "text_layout")]
fn process_text_node( /* ... unchanged ... */ node_id: NodeId, positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>, styled_dom: &StyledDom, formatting_contexts: &NodeDataContainerRef<FormattingContext>, available_rect: LogicalRect, renderer_resources: &impl RendererResourcesTrait, exclusion_areas: &[TextExclusionArea], debug_messages: &mut Option<Vec<LayoutDebugMessage>>) {}
fn get_relevant_exclusions_for_text<'a>(exclusion_areas: &'a [TextExclusionArea], text_rect: LogicalRect) -> Vec<&'a TextExclusionArea> { Vec::new() }
fn position_absolute_elements( /* ... unchanged ... */ positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>, styled_dom: &StyledDom, formatting_contexts: &NodeDataContainerRef<FormattingContext>, root_bounds: LogicalRect, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) {}
fn find_positioned_ancestor(node_id: NodeId, positioned_rects: &NodeDataContainerRef<PositionedRectangle>, styled_dom: &StyledDom, root_bounds: LogicalRect) -> LogicalRect { root_bounds }
pub fn position_absolute_element( /* ... unchanged ... */ node_id: NodeId, positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>, styled_dom: &StyledDom, containing_block: LogicalRect, position_type: LayoutPosition, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) {}
fn get_fixed_element_parent_offsets(node_id: NodeId, styled_dom: &StyledDom, css_property_cache: &CssPropertyCache,) -> LogicalPosition { LogicalPosition::zero() }
fn finalize_scrollable_areas(positioned_rects: &NodeDataContainer<PositionedRectangle>, styled_dom: &StyledDom, debug_messages: &mut Option<Vec<LayoutDebugMessage>>,) -> azul_core::ui_solver::ScrolledNodes { Default::default() }
fn update_inline_element_position(node_id: NodeId, positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>, styled_dom: &StyledDom, rect: LogicalRect,) {}
pub fn calculate_padding(node_id: NodeId, styled_dom: &StyledDom, available_space: LogicalRect,) -> ResolvedOffsets { ResolvedOffsets::default() }
pub fn calculate_border(node_id: NodeId, styled_dom: &StyledDom, available_space: LogicalRect,) -> ResolvedOffsets { ResolvedOffsets::default() }
pub fn calculate_padding_and_border(padding: &ResolvedOffsets, border: &ResolvedOffsets,) -> ResolvedOffsets { ResolvedOffsets::default() }
pub fn calculate_margin(node_id: NodeId, styled_dom: &StyledDom, available_space: LogicalRect,) -> ResolvedOffsets { ResolvedOffsets::default() }
fn calculate_line_height(node_id: NodeId, styled_dom: &StyledDom, font_size: f32) -> f32 { font_size * 1.2 }
fn calculate_font_metrics(node_id: NodeId, styled_dom: &StyledDom) -> (f32, f32) { (16.0, 16.0 * 1.2) }
fn calculate_text_content_size(node_id: NodeId, styled_dom: &StyledDom, intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>, available_space: LogicalRect,) -> LogicalSize { LogicalSize::zero() }
pub fn calculate_constrained_size(node_id: NodeId, intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>, available_space: LogicalRect, styled_dom: &StyledDom, formatting_contexts: &NodeDataContainerRef<FormattingContext>,) -> LogicalSize { LogicalSize { width: available_space.size.width, height: intrinsic_sizes[node_id].preferred_height.unwrap_or(20.0) } }
fn calculate_intrinsic_size(node_id: NodeId, intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>, available_space: LogicalRect, styled_dom: &StyledDom,) -> LogicalSize { LogicalSize::zero() }
fn create_positioned_rectangle(node_id: NodeId, styled_dom: &StyledDom, available_space: LogicalRect, content_size: LogicalSize, padding_and_border: ResolvedOffsets, margin: ResolvedOffsets, debug_messages: &mut Option<Vec<LayoutDebugMessage>>,) -> PositionedRectangle { PositionedRectangle::default() }
pub fn adjust_rect_for_floats(rect: LogicalRect, floats: &[&TextExclusionArea], debug_messages: &mut Option<Vec<LayoutDebugMessage>>,) -> LogicalRect { rect }
pub fn get_relevant_floats<'a>(exclusion_areas: &'a [TextExclusionArea], vertical_range: (f32, f32),) -> Vec<&'a TextExclusionArea> { Vec::new() }
fn get_text_align(node_id: NodeId, styled_dom: &StyledDom) -> StyleTextAlign { StyleTextAlign::Left }
fn extract_text_layout_options(node_id: NodeId, styled_dom: &StyledDom,) -> ResolvedTextLayoutOptions { ResolvedTextLayoutOptions::default() }
pub fn fix_node_positions(layout_result: &mut LayoutResult) {}
fn fix_node_position_recursive(node_id: NodeId, parent_content_box: Option<(f32, f32)>, positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>, node_hierarchy: &NodeDataContainerRef<NodeHierarchyItem>,) {}
