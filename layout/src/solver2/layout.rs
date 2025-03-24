use std::collections::{BTreeMap, BTreeSet};

use azul_core::{
    app_resources::{
        DecodedImage, ExclusionSide, RendererResourcesTrait, ShapedWords, TextExclusionArea,
        WordPositions, Words,
    },
    dom::{NodeData, NodeType},
    id_tree::{NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut, NodeId},
    styled_dom::{DomId, NodeHierarchyItem, ParentWithNodeDepth, StyleFontFamiliesHash, StyledDom},
    ui_solver::{
        FormattingContext, InlineTextLayout, InlineTextLayoutRustInternal, IntrinsicSizes,
        LayoutResult, PositionInfo, PositionInfoInner, PositionedRectangle, ResolvedOffsets,
        ResolvedTextLayoutOptions, DEFAULT_LETTER_SPACING, DEFAULT_WORD_SPACING,
    },
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::*;

use crate::text2::layout::{position_words, shape_words, split_text_into_words, HyphenationCache};

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
    let (words_cache, shaped_words_cache, positioned_words_cache) = build_text_caches(
        styled_dom,
        &positioned_rects.as_ref(),
        renderer_resources,
        debug_messages,
    );

    // Create the final LayoutResult
    LayoutResult {
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
    }
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
    use crate::text2::shaping::ParsedFont;

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
fn layout_node_recursive(
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
            let padding_and_border =
                calculate_padding_and_border(node_id, styled_dom, available_space);
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
        FormattingContext::None => {
            // Elements with display:none contribute nothing to layout
            LogicalSize::zero()
        }
    }
}

/// Handles layout for block formatting context
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
    // Get and apply size constraints
    let constrained_size =
        calculate_constrained_size(node_id, intrinsic_sizes, available_space, styled_dom);

    // Calculate padding, border, and margin
    let padding_and_border = calculate_padding_and_border(node_id, styled_dom, available_space);
    let margin = calculate_margin(node_id, styled_dom, available_space);

    // Calculate the content box position and size
    let content_box = LogicalRect::new(
        LogicalPosition::new(
            available_space.origin.x + margin.left + padding_and_border.left,
            available_space.origin.y + margin.top + padding_and_border.top,
        ),
        LogicalSize::new(
            constrained_size.width - padding_and_border.left - padding_and_border.right,
            constrained_size.height - padding_and_border.top - padding_and_border.bottom,
        ),
    );

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Block layout for node {}: available={:?}, content_box={:?}",
                node_id.index(),
                available_space,
                content_box
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
                content_box.size.height - (current_y - content_box.origin.y),
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
    let final_height = if intrinsic_sizes[node_id].preferred_height.is_some() {
        constrained_size.height
    } else {
        padding_and_border.top + height_from_children + padding_and_border.bottom
    };

    // Update the positioned rectangle for this node
    positioned_rects[node_id] = create_positioned_rectangle(
        node_id,
        styled_dom,
        available_space,
        LogicalSize::new(constrained_size.width, final_height),
        padding_and_border,
        margin,
    );

    // If this establishes a new BFC, merge the local exclusion areas
    if establishes_new_context && !local_exclusion_areas.is_empty() {
        for (id, areas) in local_exclusion_areas {
            exclusion_areas.insert(id, areas);
        }
    }

    // Return the total size including margin
    LogicalSize::new(
        constrained_size.width + margin.left + margin.right,
        final_height + margin.top + margin.bottom,
    )
}

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
    let constrained_size =
        calculate_constrained_size(node_id, intrinsic_sizes, available_space, styled_dom);

    // Calculate padding, border, and margin
    let padding_and_border = calculate_padding_and_border(node_id, styled_dom, available_space);
    let margin = calculate_margin(node_id, styled_dom, available_space);

    // Calculate the content box
    let content_box = LogicalRect::new(
        LogicalPosition::new(
            available_space.origin.x + margin.left + padding_and_border.left,
            available_space.origin.y + margin.top + padding_and_border.top,
        ),
        LogicalSize::new(
            constrained_size.width - padding_and_border.left - padding_and_border.right,
            constrained_size.height - padding_and_border.top - padding_and_border.bottom,
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

        // Skip display:none elements
        if matches!(formatting_context, FormattingContext::None) {
            continue;
        }

        // Calculate child's intrinsic size
        let child_intrinsic_size =
            calculate_intrinsic_size(child_id, intrinsic_sizes, content_box, styled_dom);

        // Calculate child's margin
        let child_margin = calculate_margin(child_id, styled_dom, content_box);

        // For InlineBlock elements, layout them first to get the correct size
        if matches!(formatting_context, FormattingContext::InlineBlock) {
            let child_rect = LogicalRect::new(
                LogicalPosition::zero(), // Actual position will be set later
                LogicalSize::new(child_intrinsic_size.width, child_intrinsic_size.height),
            );

            layout_node_recursive(
                child_id,
                positioned_rects,
                styled_dom,
                formatting_contexts,
                intrinsic_sizes,
                child_rect,
                exclusion_areas,
                debug_messages,
            );
        }

        // Add to inline elements
        inline_elements.push((
            child_id,
            child_intrinsic_size.width + child_margin.left + child_margin.right,
            child_intrinsic_size.height + child_margin.top + child_margin.bottom,
            matches!(formatting_context, FormattingContext::Inline),
        ));
    }

    // Create line boxes and distribute inline elements
    let mut line_boxes = Vec::new();
    let mut current_line = Vec::new();
    let mut current_line_width = 0.0;
    let mut current_y = content_box.origin.y;
    let mut line_height = 0.0;

    // Get available width for content, considering floats
    let mut available_content_width = content_box.size.width;

    for (child_id, width, height, is_inline) in inline_elements {
        let exclusion_refs = exclusion_areas
            .values()
            .flat_map(|v| v.iter())
            .collect::<Vec<_>>();

        // Adjust the available width for floats at the current y position
        let adjusted_rect = adjust_rect_for_floats(
            LogicalRect::new(
                LogicalPosition::new(content_box.origin.x, current_y),
                LogicalSize::new(content_box.size.width, 1.0), // Height doesn't matter here
            ),
            &exclusion_refs,
            debug_messages,
        );

        available_content_width = adjusted_rect.size.width;

        // Check if adding this element would overflow the line
        if current_line_width + width > available_content_width && !current_line.is_empty() {
            // Finish current line
            line_boxes.push((current_line, current_y, line_height));

            // Start a new line
            current_line = vec![(child_id, width, height, is_inline)];
            current_line_width = width;
            current_y += line_height;
            line_height = height;

            // Recheck for floats at the new y position
            let adjusted_rect = adjust_rect_for_floats(
                LogicalRect::new(
                    LogicalPosition::new(content_box.origin.x, current_y),
                    LogicalSize::new(content_box.size.width, 1.0),
                ),
                &exclusion_refs,
                debug_messages,
            );

            available_content_width = adjusted_rect.size.width;
        } else {
            // Add to current line
            current_line.push((child_id, width, height, is_inline));
            current_line_width += width;
            line_height = line_height.max(height);
        }
    }

    // Add the last line if not empty
    if !current_line.is_empty() {
        line_boxes.push((current_line, current_y, line_height));
        current_y += line_height;
    }

    // Position all inline elements within their line boxes
    for (line, y_position, height) in line_boxes {
        let mut current_x = content_box.origin.x;

        let exclusion_refs = exclusion_areas
            .values()
            .flat_map(|v| v.iter())
            .collect::<Vec<_>>();

        // Adjust starting position for floats
        let adjusted_rect = adjust_rect_for_floats(
            LogicalRect::new(
                LogicalPosition::new(content_box.origin.x, y_position),
                LogicalSize::new(content_box.size.width, height),
            ),
            &exclusion_refs,
            debug_messages,
        );

        current_x = adjusted_rect.origin.x;

        // Get text alignment to determine element positioning within line
        let text_align = get_text_align(node_id, styled_dom);

        // Calculate total width of line elements for alignment
        let line_total_width: f32 = line.iter().map(|(_, width, _, _)| *width).sum();

        // Adjust starting x based on text alignment
        match text_align {
            StyleTextAlign::Left => {
                // Left alignment - default, no adjustment needed
            }
            StyleTextAlign::Right => {
                // Right alignment - shift to the right
                current_x += adjusted_rect.size.width - line_total_width;
            }
            StyleTextAlign::Center => {
                // Center alignment
                current_x += (adjusted_rect.size.width - line_total_width) / 2.0;
            }
            StyleTextAlign::Justify => {
                // Justify - only distribute space if not the last line
                // For simplicity, not implementing justification here
            }
        }

        for (child_id, width, height, is_inline) in line {
            // Get child's margin
            let child_margin = calculate_margin(child_id, styled_dom, content_box);

            // Skip to margin left
            current_x += child_margin.left;

            // Calculate child's available space
            let child_space = LogicalRect::new(
                LogicalPosition::new(current_x, y_position + child_margin.top),
                LogicalSize::new(
                    width - child_margin.left - child_margin.right,
                    height - child_margin.top - child_margin.bottom,
                ),
            );

            // For non-inline elements (like inline-block), recursively calculate layout
            if !is_inline {
                layout_node_recursive(
                    child_id,
                    positioned_rects,
                    styled_dom,
                    formatting_contexts,
                    intrinsic_sizes,
                    child_space,
                    exclusion_areas,
                    debug_messages,
                );
            }

            // Update positioned rectangle for this node
            update_inline_element_position(child_id, positioned_rects, styled_dom, child_space);

            // Move to next element position
            current_x += width - child_margin.left;
        }
    }

    // Calculate final height including all line boxes
    let final_height = if intrinsic_sizes[node_id].preferred_height.is_some() {
        constrained_size.height
    } else {
        padding_and_border.top + (current_y - content_box.origin.y) + padding_and_border.bottom
    };

    // Update positioned rectangle for this node
    positioned_rects[node_id] = create_positioned_rectangle(
        node_id,
        styled_dom,
        available_space,
        LogicalSize::new(constrained_size.width, final_height),
        padding_and_border,
        margin,
    );

    // Return the total size including margin
    LogicalSize::new(
        constrained_size.width + margin.left + margin.right,
        final_height + margin.top + margin.bottom,
    )
}

/// Layout elements in a flex formatting context
fn layout_flex_context(
    node_id: NodeId,
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    styled_dom: &StyledDom,
    formatting_contexts: &NodeDataContainerRef<FormattingContext>,
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
    available_space: LogicalRect,
    exclusion_areas: &mut BTreeMap<NodeId, Vec<TextExclusionArea>>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LogicalSize {
    // For simplicity, implement a very basic flexbox layout
    // In a real implementation, this would be much more complex

    // Apply size constraints
    let constrained_size =
        calculate_constrained_size(node_id, intrinsic_sizes, available_space, styled_dom);

    // Calculate padding, border, and margin
    let padding_and_border = calculate_padding_and_border(node_id, styled_dom, available_space);
    let margin = calculate_margin(node_id, styled_dom, available_space);

    // Calculate the content box
    let content_box = LogicalRect::new(
        LogicalPosition::new(
            available_space.origin.x + margin.left + padding_and_border.left,
            available_space.origin.y + margin.top + padding_and_border.top,
        ),
        LogicalSize::new(
            constrained_size.width - padding_and_border.left - padding_and_border.right,
            constrained_size.height - padding_and_border.top - padding_and_border.bottom,
        ),
    );

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Flex layout for node {}: available={:?}, content_box={:?}",
                node_id.index(),
                available_space,
                content_box
            )
            .into(),
            location: "layout_flex_context".to_string().into(),
        });
    }

    // Get flex direction
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;
    let flex_direction = css_property_cache
        .get_flex_direction(node_data, &node_id, styled_node_state)
        .and_then(|dir| dir.get_property().copied())
        .unwrap_or_default();

    let is_row = flex_direction.get_axis() == LayoutAxis::Horizontal;

    // Collect flex items (excluding display:none)
    let mut flex_items = Vec::new();

    for child_id in node_id.az_children(&styled_dom.node_hierarchy.as_container()) {
        let formatting_context = &formatting_contexts[child_id];

        // Skip display:none elements
        if matches!(formatting_context, FormattingContext::None) {
            continue;
        }

        // Get flex-grow factor
        let flex_grow = css_property_cache
            .get_flex_grow(node_data, &child_id, styled_node_state)
            .and_then(|fg| fg.get_property().copied())
            .map_or(0.0, |fg| fg.inner.get().max(0.0));

        // Calculate child's intrinsic size
        let child_intrinsic_size =
            calculate_intrinsic_size(child_id, intrinsic_sizes, content_box, styled_dom);

        flex_items.push((child_id, child_intrinsic_size, flex_grow));
    }

    // Calculate total flex size and flex-grow factors
    let mut total_main_size = 0.0;
    let mut total_flex_grow = 0.0;

    for (_, size, flex_grow) in &flex_items {
        if is_row {
            total_main_size += size.width;
        } else {
            total_main_size += size.height;
        }
        total_flex_grow += *flex_grow;
    }

    // Calculate remaining space
    let main_axis_size = if is_row {
        content_box.size.width
    } else {
        content_box.size.height
    };

    let remaining_space = (main_axis_size - total_main_size).max(0.0);

    // Layout flex items
    let mut current_main_pos = if is_row {
        content_box.origin.x
    } else {
        content_box.origin.y
    };

    let cross_start = if is_row {
        content_box.origin.y
    } else {
        content_box.origin.x
    };

    let cross_size = if is_row {
        content_box.size.height
    } else {
        content_box.size.width
    };

    for (child_id, mut size, flex_grow) in flex_items {
        // Apply flex-grow
        let extra_space = if total_flex_grow > 0.0 {
            (flex_grow / total_flex_grow) * remaining_space
        } else {
            0.0
        };

        if is_row {
            size.width += extra_space;
        } else {
            size.height += extra_space;
        }

        // Position the flex item
        let child_rect = if is_row {
            LogicalRect::new(LogicalPosition::new(current_main_pos, cross_start), size)
        } else {
            LogicalRect::new(LogicalPosition::new(cross_start, current_main_pos), size)
        };

        // Layout the child
        layout_node_recursive(
            child_id,
            positioned_rects,
            styled_dom,
            formatting_contexts,
            intrinsic_sizes,
            child_rect,
            exclusion_areas,
            debug_messages,
        );

        // Move main axis position
        if is_row {
            current_main_pos += size.width;
        } else {
            current_main_pos += size.height;
        }
    }

    // Update positioned rectangle for this node
    positioned_rects[node_id] = create_positioned_rectangle(
        node_id,
        styled_dom,
        available_space,
        LogicalSize::new(constrained_size.width, constrained_size.height),
        padding_and_border,
        margin,
    );

    // Return the total size including margin
    LogicalSize::new(
        constrained_size.width + margin.left + margin.right,
        constrained_size.height + margin.top + margin.bottom,
    )
}

/// Layout a floating element
fn layout_float(
    node_id: NodeId,
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    styled_dom: &StyledDom,
    formatting_contexts: &NodeDataContainerRef<FormattingContext>,
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
    available_space: LogicalRect,
    float_direction: LayoutFloat,
    exclusion_areas: &mut BTreeMap<NodeId, Vec<TextExclusionArea>>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LogicalSize {
    // Apply size constraints
    let constrained_size =
        calculate_constrained_size(node_id, intrinsic_sizes, available_space, styled_dom);

    // Calculate padding, border, and margin
    let padding_and_border = calculate_padding_and_border(node_id, styled_dom, available_space);
    let margin = calculate_margin(node_id, styled_dom, available_space);

    // Calculate the float's position
    let mut float_position = LogicalPosition::new(
        available_space.origin.x,
        available_space.origin.y + margin.top,
    );

    // Adjust position based on float direction
    match float_direction {
        LayoutFloat::Left => {
            float_position.x += margin.left;

            // Find the lowest point where this float can be placed
            // (considering existing floats)
            float_position = find_float_position(
                float_position,
                constrained_size,
                float_direction,
                exclusion_areas,
                node_id,
            );
        }
        LayoutFloat::Right => {
            float_position.x = available_space.origin.x + available_space.size.width
                - constrained_size.width
                - margin.right;

            // Find the lowest point where this float can be placed
            float_position = find_float_position(
                float_position,
                constrained_size,
                float_direction,
                exclusion_areas,
                node_id,
            );
        }
        LayoutFloat::None => {
            // Not actually a float, but handle it anyway
        }
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Float layout for node {}: direction={:?}, position={:?}, size={:?}",
                node_id.index(),
                float_direction,
                float_position,
                constrained_size
            )
            .into(),
            location: "layout_float".to_string().into(),
        });
    }

    // Calculate the content box
    let content_box = LogicalRect::new(
        LogicalPosition::new(
            float_position.x + padding_and_border.left,
            float_position.y + padding_and_border.top,
        ),
        LogicalSize::new(
            constrained_size.width - padding_and_border.left - padding_and_border.right,
            constrained_size.height - padding_and_border.top - padding_and_border.bottom,
        ),
    );

    // Layout children
    let mut max_child_height = 0.0_f32;

    for child_id in node_id.az_children(&styled_dom.node_hierarchy.as_container()) {
        let child_size = layout_node_recursive(
            child_id,
            positioned_rects,
            styled_dom,
            formatting_contexts,
            intrinsic_sizes,
            content_box,
            exclusion_areas,
            debug_messages,
        );

        max_child_height = max_child_height.max(child_size.height);
    }

    // Adjust height if needed
    let final_height = if intrinsic_sizes[node_id].preferred_height.is_some() {
        constrained_size.height
    } else {
        (padding_and_border.top + max_child_height + padding_and_border.bottom)
            .max(constrained_size.height)
    };

    // Create exclusion area for this float
    let exclusion = TextExclusionArea {
        rect: LogicalRect::new(
            float_position,
            LogicalSize::new(constrained_size.width, final_height),
        ),
        side: match float_direction {
            LayoutFloat::Left => ExclusionSide::Left,
            LayoutFloat::Right => ExclusionSide::Right,
            LayoutFloat::None => ExclusionSide::None,
        },
    };

    // Add to exclusion areas
    exclusion_areas
        .entry(node_id)
        .or_insert_with(Vec::new)
        .push(exclusion);

    // Update positioned rectangle for this node
    positioned_rects[node_id] = create_positioned_rectangle(
        node_id,
        styled_dom,
        LogicalRect::new(float_position, constrained_size),
        LogicalSize::new(constrained_size.width, final_height),
        padding_and_border,
        margin,
    );

    // Return the total size including margin
    LogicalSize::new(
        constrained_size.width + margin.left + margin.right,
        final_height + margin.top + margin.bottom,
    )
}

/// Find a position for a floating element
fn find_float_position(
    initial_position: LogicalPosition,
    size: LogicalSize,
    float_direction: LayoutFloat,
    exclusion_areas: &BTreeMap<NodeId, Vec<TextExclusionArea>>,
    current_node_id: NodeId,
) -> LogicalPosition {
    let mut position = initial_position;

    // Check for intersection with any existing exclusion areas
    for (node_id, areas) in exclusion_areas {
        // Skip exclusions from the current node
        if *node_id == current_node_id {
            continue;
        }

        for area in areas {
            let float_rect = LogicalRect::new(position, size);

            if float_rect.intersects(&area.rect) {
                // Move down below this exclusion area
                position.y = area.rect.origin.y + area.rect.size.height;
            }
        }
    }

    position
}

/// Process text layout and handle inline elements
fn process_text_layout(
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    styled_dom: &StyledDom,
    formatting_contexts: &NodeDataContainerRef<FormattingContext>,
    renderer_resources: &impl RendererResourcesTrait,
    exclusion_areas: &[TextExclusionArea],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) {
    // Find all text nodes
    let mut text_nodes = Vec::new();

    for (i, node_data) in styled_dom.node_data.as_container().iter().enumerate() {
        if matches!(node_data.get_node_type(), NodeType::Text(_)) {
            text_nodes.push(NodeId::new(i));
        }
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Processing text layout for {} text nodes", text_nodes.len()).into(),
            location: "process_text_layout".to_string().into(),
        });
    }

    // Process each text node
    for node_id in text_nodes {
        let rect = &positioned_rects[node_id];

        // Skip nodes with zero width or height
        if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
            continue;
        }

        // Create available rect for text layout
        let available_rect = LogicalRect::new(rect.position.get_static_offset(), rect.size);

        // Process the text node
        process_text_node(
            node_id,
            positioned_rects,
            styled_dom,
            formatting_contexts,
            available_rect,
            renderer_resources,
            exclusion_areas,
            debug_messages,
        );
    }
}

/// Process a text node for layout
fn process_text_node(
    node_id: NodeId,
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    styled_dom: &StyledDom,
    formatting_contexts: &NodeDataContainerRef<FormattingContext>,
    available_rect: LogicalRect,
    renderer_resources: &impl RendererResourcesTrait,
    exclusion_areas: &[TextExclusionArea],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) {
    use azul_core::ui_solver::ScriptType;

    use crate::text2::{
        layout::{
            layout_text_node as text2_layout_text_node, position_words, shape_words,
            split_text_into_words_with_hyphenation, HyphenationCache,
        },
        shaping::ParsedFont,
    };

    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Get text content from node
    let text = match node_data.get_node_type() {
        NodeType::Text(text_content) => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: format!(
                        "Processing text node {}: '{}'",
                        node_id.index(),
                        if text_content.len() > 30 {
                            &text_content.as_str()[0..30]
                        } else {
                            text_content.as_str()
                        }
                    )
                    .into(),
                    location: "process_text_node".to_string().into(),
                });
            }
            text_content.as_str()
        }
        _ => return, // Not a text node
    };

    // Get CSS property cache and node state
    let css_property_cache = styled_dom.get_css_property_cache();
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    // Calculate padding and margins
    let padding = calculate_padding_and_border(node_id, styled_dom, available_rect);
    let margin = calculate_margin(node_id, styled_dom, available_rect);

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Text node {} padding: {:?}, margin: {:?}",
                node_id.index(),
                padding,
                margin
            )
            .into(),
            location: "process_text_node".to_string().into(),
        });
    }

    // Adjust available rect for padding and margin
    let content_rect = LogicalRect::new(
        LogicalPosition::new(
            available_rect.origin.x + margin.left + padding.left,
            available_rect.origin.y + margin.top + padding.top,
        ),
        LogicalSize::new(
            available_rect.size.width - padding.left - padding.right - margin.left - margin.right,
            available_rect.size.height - padding.top - padding.bottom - margin.top - margin.bottom,
        ),
    );

    // Extract text styling properties
    let font_families =
        css_property_cache.get_font_id_or_default(node_data, &node_id, styled_node_state);
    let font_size =
        css_property_cache.get_font_size_or_default(node_data, &node_id, styled_node_state);

    // Get text direction
    let direction = css_property_cache
        .get_direction(node_data, &node_id, styled_node_state)
        .and_then(|dir| dir.get_property().copied())
        .unwrap_or_default();

    let is_rtl = if direction == StyleDirection::Rtl {
        ScriptType::RTL
    } else {
        ScriptType::LTR
    };

    // Get text alignment
    let text_align = css_property_cache
        .get_text_align(node_data, &node_id, styled_node_state)
        .and_then(|ta| ta.get_property().copied())
        .unwrap_or(StyleTextAlign::Left);

    // Get the font from the renderer resources
    let css_font_families_hash = StyleFontFamiliesHash::new(font_families.as_ref());
    let css_font_family = match renderer_resources.get_font_family(&css_font_families_hash) {
        Some(f) => f,
        None => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: format!("Font family not found for node {}", node_id.index()).into(),
                    location: "process_text_node".to_string().into(),
                });
            }
            return;
        }
    };

    let font_key = match renderer_resources.get_font_key(css_font_family) {
        Some(k) => k,
        None => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: format!("Font key not found for node {}", node_id.index()).into(),
                    location: "process_text_node".to_string().into(),
                });
            }
            return;
        }
    };

    let (font_ref, _) = match renderer_resources.get_registered_font(font_key) {
        Some(fr) => fr,
        None => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: format!("Font reference not found for node {}", node_id.index())
                        .into(),
                    location: "process_text_node".to_string().into(),
                });
            }
            return;
        }
    };

    // Get the parsed font
    let font_data = font_ref.get_data();
    let parsed_font = unsafe { &*(font_data.parsed as *const ParsedFont) };

    // Create text layout options
    let line_height = css_property_cache
        .get_line_height(node_data, &node_id, styled_node_state)
        .and_then(|lh| Some(lh.get_property()?.inner.normalized()));

    let letter_spacing = css_property_cache
        .get_letter_spacing(node_data, &node_id, styled_node_state)
        .and_then(|ls| {
            Some(
                ls.get_property()?
                    .inner
                    .to_pixels(font_size.inner.to_pixels(100.0)),
            )
        });

    let word_spacing = css_property_cache
        .get_word_spacing(node_data, &node_id, styled_node_state)
        .and_then(|ws| {
            Some(
                ws.get_property()?
                    .inner
                    .to_pixels(font_size.inner.to_pixels(100.0)),
            )
        });

    let tab_width = css_property_cache
        .get_tab_width(node_data, &node_id, styled_node_state)
        .and_then(|tw| Some(tw.get_property()?.inner.normalized()));

    // Create resolved text layout options
    let text_layout_options = ResolvedTextLayoutOptions {
        font_size_px: font_size.inner.to_pixels(100.0), // 100.0 is reference size for percentage
        line_height: line_height.into(),
        letter_spacing: letter_spacing.into(),
        word_spacing: word_spacing.into(),
        tab_width: tab_width.into(),
        max_horizontal_width: Some(content_rect.size.width).into(),
        max_vertical_height: None.into(), // Some(content_rect.size.height).into(),
        leading: None.into(),
        holes: Vec::new().into(),
        can_break: true,
        can_hyphenate: true,
        hyphenation_character: Some('-' as u32).into(),
        is_rtl,
        text_justify: Some(text_align).into(),
    };

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Created text layout options for node {}: font_size={}, is_rtl={:?}",
                node_id.index(),
                text_layout_options.font_size_px,
                text_layout_options.is_rtl,
            )
            .into(),
            location: "process_text_node".to_string().into(),
        });
    }

    // Process text with hyphenation
    // Use a static HyphenationCache to avoid recreating it for each text node
    static mut HYPHENATION_CACHE: Option<HyphenationCache> = None;
    let hyphenation_cache = unsafe {
        if HYPHENATION_CACHE.is_none() {
            HYPHENATION_CACHE = Some(HyphenationCache::new());
        }
        HYPHENATION_CACHE.as_ref().unwrap()
    };

    // Split text into words, with hyphenation if enabled
    let words = split_text_into_words_with_hyphenation(
        text,
        &text_layout_options,
        hyphenation_cache,
        debug_messages,
    );

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Split text into {} words for node {}",
                words.items.len(),
                node_id.index(),
            )
            .into(),
            location: "process_text_node".to_string().into(),
        });
    }

    // Shape the words using the font
    let shaped_words = shape_words(&words, parsed_font);

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Shaped text for node {}: {} shaped words",
                node_id.index(),
                shaped_words.items.len(),
            )
            .into(),
            location: "process_text_node".to_string().into(),
        });
    }

    // Process the layout for text with exclusion areas (floats)
    let relevant_exclusions = get_relevant_exclusions_for_text(exclusion_areas, content_rect);

    // Position the words based on the layout options and exclusions
    let word_positions =
        position_words(&words, &shaped_words, &text_layout_options, debug_messages);

    // Convert word positions to inline text layout
    let mut inline_text_layout =
        crate::text2::layout::word_positions_to_inline_text_layout(&word_positions);

    // Apply text alignment if needed and not already handled by position_words
    if text_align != StyleTextAlign::Left {
        inline_text_layout.align_children_horizontal(&content_rect.size, text_align);
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Text layout for node {} complete: {} lines, content size: {:?}",
                node_id.index(),
                inline_text_layout.lines.len(),
                inline_text_layout.content_size,
            )
            .into(),
            location: "process_text_node".to_string().into(),
        });
    }

    // Update the positioned rectangle with the text layout
    let mut rect_mut = positioned_rects[node_id].clone();
    rect_mut.resolved_text_layout_options = Some((text_layout_options, inline_text_layout.into()));
    positioned_rects[node_id] = rect_mut;
}

/// Get exclusion areas relevant for text layout
fn get_relevant_exclusions_for_text<'a>(
    exclusion_areas: &'a [TextExclusionArea],
    text_rect: LogicalRect,
) -> Vec<&'a TextExclusionArea> {
    // Filter exclusion areas that could affect this text node
    exclusion_areas
        .iter()
        .filter(|area| {
            // Check if the exclusion area overlaps vertically with the text rectangle
            let area_top = area.rect.origin.y;
            let area_bottom = area.rect.origin.y + area.rect.size.height;
            let text_top = text_rect.origin.y;
            let text_bottom = text_rect.origin.y + text_rect.size.height;

            // Exclusion affects text if there's any vertical overlap
            (area_top <= text_bottom && area_bottom >= text_top)
        })
        .collect()
}

/// Position absolutely positioned elements
fn position_absolute_elements(
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    styled_dom: &StyledDom,
    formatting_contexts: &NodeDataContainerRef<FormattingContext>,
    root_bounds: LogicalRect,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) {
    // Find all absolutely positioned elements
    let mut absolute_elements = Vec::new();

    for (i, fc) in formatting_contexts.internal.iter().enumerate() {
        if let FormattingContext::OutOfFlow(position) = fc {
            if *position == LayoutPosition::Absolute || *position == LayoutPosition::Fixed {
                absolute_elements.push((NodeId::new(i), *position));
            }
        }
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Positioning {} absolutely positioned elements",
                absolute_elements.len()
            )
            .into(),
            location: "position_absolute_elements".to_string().into(),
        });
    }

    // Process each absolutely positioned element
    for (node_id, position_type) in absolute_elements {
        // Find the containing block
        let containing_block = if position_type == LayoutPosition::Fixed {
            // For fixed positioning, the containing block is the viewport
            root_bounds
        } else {
            // For absolute positioning, the containing block is the nearest positioned ancestor
            find_positioned_ancestor(
                node_id,
                &positioned_rects.as_borrowing_ref(),
                styled_dom,
                root_bounds,
            )
        };

        // Position the element within its containing block
        position_absolute_element(
            node_id,
            positioned_rects,
            styled_dom,
            containing_block,
            position_type,
            debug_messages,
        );
    }
}

/// Find the positioned ancestor for an absolutely positioned element
fn find_positioned_ancestor(
    node_id: NodeId,
    positioned_rects: &NodeDataContainerRef<PositionedRectangle>,
    styled_dom: &StyledDom,
    root_bounds: LogicalRect,
) -> LogicalRect {
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let mut current_id = node_hierarchy[node_id].parent_id();

    while let Some(parent_id) = current_id {
        // Check if this parent has a positioned formatting context
        let is_positioned = match &positioned_rects[parent_id].position {
            PositionInfo::Relative(_) | PositionInfo::Absolute(_) | PositionInfo::Fixed(_) => true,
            PositionInfo::Static(_) => false,
        };

        if is_positioned {
            // Found a positioned ancestor
            return LogicalRect::new(
                positioned_rects[parent_id].position.get_static_offset(),
                positioned_rects[parent_id].size,
            );
        }

        // Move up to the next parent
        current_id = node_hierarchy[parent_id].parent_id();
    }

    // If no positioned ancestor was found, use the root bounds
    root_bounds
}

/// Position an absolutely positioned element
fn position_absolute_element(
    node_id: NodeId,
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    styled_dom: &StyledDom,
    containing_block: LogicalRect,
    position_type: LayoutPosition,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    // Get the element's current size (calculated during regular layout)
    let element_size = positioned_rects[node_id].size;

    // Get CSS positioning properties (left, right, top, bottom)
    let left = css_property_cache
        .get_left(node_data, &node_id, styled_node_state)
        .and_then(|l| {
            Some(
                l.get_property()?
                    .inner
                    .to_pixels(containing_block.size.width),
            )
        });

    let right = css_property_cache
        .get_right(node_data, &node_id, styled_node_state)
        .and_then(|r| {
            Some(
                r.get_property()?
                    .inner
                    .to_pixels(containing_block.size.width),
            )
        });

    let top = css_property_cache
        .get_top(node_data, &node_id, styled_node_state)
        .and_then(|t| {
            Some(
                t.get_property()?
                    .inner
                    .to_pixels(containing_block.size.height),
            )
        });

    let bottom = css_property_cache
        .get_bottom(node_data, &node_id, styled_node_state)
        .and_then(|b| {
            Some(
                b.get_property()?
                    .inner
                    .to_pixels(containing_block.size.height),
            )
        });

    // Calculate the position
    let mut position = LogicalPosition::new(containing_block.origin.x, containing_block.origin.y);

    // Apply horizontal positioning (left/right)
    if let Some(left_value) = left {
        position.x += left_value;
    } else if let Some(right_value) = right {
        position.x = containing_block.origin.x + containing_block.size.width
            - element_size.width
            - right_value;
    }

    // Apply vertical positioning (top/bottom)
    if let Some(top_value) = top {
        position.y += top_value;
    } else if let Some(bottom_value) = bottom {
        position.y = containing_block.origin.y + containing_block.size.height
            - element_size.height
            - bottom_value;
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Positioned absolute element {}: position={:?}, size={:?}, type={:?}",
                node_id.index(),
                position,
                element_size,
                position_type
            )
            .into(),
            location: "position_absolute_element".to_string().into(),
        });
    }

    // Update the positioned rectangle
    let mut rect = positioned_rects[node_id].clone();
    rect.position = match position_type {
        LayoutPosition::Absolute => PositionInfo::Absolute(PositionInfoInner {
            x_offset: position.x - containing_block.origin.x,
            y_offset: position.y - containing_block.origin.y,
            static_x_offset: position.x,
            static_y_offset: position.y,
        }),
        LayoutPosition::Fixed => PositionInfo::Fixed(PositionInfoInner {
            x_offset: position.x,
            y_offset: position.y,
            static_x_offset: position.x,
            static_y_offset: position.y,
        }),
        _ => rect.position, // Shouldn't happen
    };
    positioned_rects[node_id] = rect;
}

/// Finalize scrollable areas
fn finalize_scrollable_areas(
    positioned_rects: &NodeDataContainer<PositionedRectangle>,
    styled_dom: &StyledDom,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> azul_core::ui_solver::ScrolledNodes {
    // In a real implementation, this would identify elements with overflow: auto/scroll
    // and calculate their scrollable area.

    // For now, we'll return an empty ScrolledNodes
    Default::default()
}

/// Update the position of an inline element
fn update_inline_element_position(
    node_id: NodeId,
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    styled_dom: &StyledDom,
    rect: LogicalRect,
) {
    let mut element_rect = positioned_rects[node_id].clone();

    // Update position
    element_rect.position = PositionInfo::Static(PositionInfoInner {
        x_offset: rect.origin.x,
        y_offset: rect.origin.y,
        static_x_offset: rect.origin.x,
        static_y_offset: rect.origin.y,
    });

    // Update size
    element_rect.size = rect.size;

    positioned_rects[node_id] = element_rect;
}

/// Calculate padding and border for a node
fn calculate_padding_and_border(
    node_id: NodeId,
    styled_dom: &StyledDom,
    available_space: LogicalRect,
) -> ResolvedOffsets {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    let parent_width = available_space.size.width;
    let parent_height = available_space.size.height;

    // Get padding
    let padding_left = css_property_cache
        .get_padding_left(node_data, &node_id, styled_node_state)
        .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(0.0);

    let padding_right = css_property_cache
        .get_padding_right(node_data, &node_id, styled_node_state)
        .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(0.0);

    let padding_top = css_property_cache
        .get_padding_top(node_data, &node_id, styled_node_state)
        .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(0.0);

    let padding_bottom = css_property_cache
        .get_padding_bottom(node_data, &node_id, styled_node_state)
        .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(0.0);

    // Get border
    let border_left = css_property_cache
        .get_border_left_width(node_data, &node_id, styled_node_state)
        .and_then(|b| Some(b.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(0.0);

    let border_right = css_property_cache
        .get_border_right_width(node_data, &node_id, styled_node_state)
        .and_then(|b| Some(b.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(0.0);

    let border_top = css_property_cache
        .get_border_top_width(node_data, &node_id, styled_node_state)
        .and_then(|b| Some(b.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(0.0);

    let border_bottom = css_property_cache
        .get_border_bottom_width(node_data, &node_id, styled_node_state)
        .and_then(|b| Some(b.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(0.0);

    ResolvedOffsets {
        left: padding_left + border_left,
        right: padding_right + border_right,
        top: padding_top + border_top,
        bottom: padding_bottom + border_bottom,
    }
}

/// Calculate margin for a node
fn calculate_margin(
    node_id: NodeId,
    styled_dom: &StyledDom,
    available_space: LogicalRect,
) -> ResolvedOffsets {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    let parent_width = available_space.size.width;
    let parent_height = available_space.size.height;

    // Get margin
    let margin_left = css_property_cache
        .get_margin_left(node_data, &node_id, styled_node_state)
        .and_then(|m| Some(m.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(0.0);

    let margin_right = css_property_cache
        .get_margin_right(node_data, &node_id, styled_node_state)
        .and_then(|m| Some(m.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(0.0);

    let margin_top = css_property_cache
        .get_margin_top(node_data, &node_id, styled_node_state)
        .and_then(|m| Some(m.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(0.0);

    let margin_bottom = css_property_cache
        .get_margin_bottom(node_data, &node_id, styled_node_state)
        .and_then(|m| Some(m.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(0.0);

    ResolvedOffsets {
        left: margin_left,
        right: margin_right,
        top: margin_top,
        bottom: margin_bottom,
    }
}

/// Calculate intrinsic size with constraints
pub fn calculate_constrained_size(
    node_id: NodeId,
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
    available_space: LogicalRect,
    styled_dom: &StyledDom,
) -> LogicalSize {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    let parent_width = available_space.size.width;
    let parent_height = available_space.size.height;

    // Get width constraints
    let width = css_property_cache
        .get_width(node_data, &node_id, styled_node_state)
        .and_then(|w| Some(w.get_property()?.inner.to_pixels(parent_width)));

    let min_width = css_property_cache
        .get_min_width(node_data, &node_id, styled_node_state)
        .and_then(|w| Some(w.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(0.0);

    let max_width = css_property_cache
        .get_max_width(node_data, &node_id, styled_node_state)
        .and_then(|w| Some(w.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(f32::MAX);

    // Get height constraints
    let height = css_property_cache
        .get_height(node_data, &node_id, styled_node_state)
        .and_then(|h| Some(h.get_property()?.inner.to_pixels(parent_height)));

    let min_height = css_property_cache
        .get_min_height(node_data, &node_id, styled_node_state)
        .and_then(|h| Some(h.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(0.0);

    let max_height = css_property_cache
        .get_max_height(node_data, &node_id, styled_node_state)
        .and_then(|h| Some(h.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(f32::MAX);

    // Get intrinsic sizes
    let intrinsic_size = &intrinsic_sizes[node_id];

    // Calculate final width
    let final_width = match width {
        Some(w) => w.max(min_width).min(max_width),
        None => match intrinsic_size.preferred_width {
            Some(preferred) => preferred.max(min_width).min(max_width),
            None => intrinsic_size
                .max_content_width
                .max(min_width)
                .min(max_width)
                .min(parent_width),
        },
    };

    // Calculate final height
    let final_height = match height {
        Some(h) => h.max(min_height).min(max_height),
        None => match intrinsic_size.preferred_height {
            Some(preferred) => preferred.max(min_height).min(max_height),
            None => intrinsic_size
                .max_content_height
                .max(min_height)
                .min(max_height),
        },
    };

    LogicalSize::new(final_width, final_height)
}

/// Calculate intrinsic size without applying constraints
fn calculate_intrinsic_size(
    node_id: NodeId,
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
    available_space: LogicalRect,
    styled_dom: &StyledDom,
) -> LogicalSize {
    let intrinsic_size = &intrinsic_sizes[node_id];

    let width = intrinsic_size
        .preferred_width
        .unwrap_or_else(|| intrinsic_size.max_content_width);

    let height = intrinsic_size
        .preferred_height
        .unwrap_or_else(|| intrinsic_size.max_content_height);

    LogicalSize::new(width, height)
}

/// Create a positioned rectangle
fn create_positioned_rectangle(
    node_id: NodeId,
    styled_dom: &StyledDom,
    available_space: LogicalRect,
    size: LogicalSize,
    padding_and_border: ResolvedOffsets,
    margin: ResolvedOffsets,
) -> PositionedRectangle {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    // Get box sizing mode
    let box_sizing = css_property_cache
        .get_box_sizing(node_data, &node_id, styled_node_state)
        .and_then(|bs| bs.get_property().copied())
        .unwrap_or_default();

    // Get overflow
    let overflow_x = css_property_cache
        .get_overflow_x(node_data, &node_id, styled_node_state)
        .and_then(|o| o.get_property().copied())
        .unwrap_or_default();

    let overflow_y = css_property_cache
        .get_overflow_y(node_data, &node_id, styled_node_state)
        .and_then(|o| o.get_property().copied())
        .unwrap_or_default();

    // Calculate position
    let position = PositionInfo::Static(PositionInfoInner {
        x_offset: available_space.origin.x - available_space.origin.x,
        y_offset: available_space.origin.y - available_space.origin.y,
        static_x_offset: available_space.origin.x,
        static_y_offset: available_space.origin.y,
    });

    PositionedRectangle {
        size,
        position,
        padding: padding_and_border,
        margin,
        border_widths: ResolvedOffsets::default(), // Would need to extract from padding_and_border
        box_shadow: Default::default(),
        box_sizing,
        resolved_text_layout_options: None,
        overflow_x,
        overflow_y,
    }
}

/// Adjust a rectangle to account for floats
pub fn adjust_rect_for_floats(
    rect: LogicalRect,
    floats: &[&TextExclusionArea],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LogicalRect {
    if floats.is_empty() {
        return rect;
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Adjusting rect {:?} for {} floats", rect, floats.len()).into(),
            location: "adjust_rect_for_floats".to_string().into(),
        });
    }

    let mut adjusted_rect = rect;

    for float in floats {
        // Check if this float affects the current line vertically
        if float.rect.origin.y <= rect.origin.y + rect.size.height
            && float.rect.origin.y + float.rect.size.height >= rect.origin.y
        {
            match float.side {
                azul_core::app_resources::ExclusionSide::Left => {
                    // Left float - adjust left edge of line
                    let float_right = float.rect.origin.x + float.rect.size.width;
                    if float_right > adjusted_rect.origin.x {
                        let new_width =
                            adjusted_rect.size.width - (float_right - adjusted_rect.origin.x);
                        adjusted_rect.origin.x = float_right;
                        adjusted_rect.size.width = new_width.max(0.0);
                    }
                }
                azul_core::app_resources::ExclusionSide::Right => {
                    // Right float - adjust right edge of line
                    let float_left = float.rect.origin.x;
                    if float_left < adjusted_rect.origin.x + adjusted_rect.size.width {
                        adjusted_rect.size.width = (float_left - adjusted_rect.origin.x).max(0.0);
                    }
                }
                azul_core::app_resources::ExclusionSide::Both => {
                    // Affects both sides - handle as a "hole" in the content
                    let float_left = float.rect.origin.x;
                    let float_right = float.rect.origin.x + float.rect.size.width;

                    // If the float intersects the line
                    if float_right > adjusted_rect.origin.x
                        && float_left < adjusted_rect.origin.x + adjusted_rect.size.width
                    {
                        // Calculate available space on both sides
                        let left_space = float_left - adjusted_rect.origin.x;
                        let right_space =
                            adjusted_rect.origin.x + adjusted_rect.size.width - float_right;

                        if left_space > right_space {
                            // More space on the left
                            adjusted_rect.size.width = left_space.max(0.0);
                        } else {
                            // More space on the right
                            adjusted_rect.origin.x = float_right;
                            adjusted_rect.size.width = right_space.max(0.0);
                        }
                    }
                }
                azul_core::app_resources::ExclusionSide::None => {
                    // No effect on the line
                }
            }
        }
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Adjusted rect for floats: original={:?}, adjusted={:?}",
                rect, adjusted_rect
            )
            .into(),
            location: "adjust_rect_for_floats".to_string().into(),
        });
    }

    adjusted_rect
}

/// Collects all relevant float exclusions affecting a specific vertical region
pub fn get_relevant_floats<'a>(
    exclusion_areas: &'a [TextExclusionArea],
    vertical_range: (f32, f32),
) -> Vec<&'a TextExclusionArea> {
    let (min_y, max_y) = vertical_range;

    exclusion_areas
        .iter()
        .filter(|area| {
            let area_top = area.rect.origin.y;
            let area_bottom = area.rect.origin.y + area.rect.size.height;

            // Check if the float overlaps with the vertical range
            (area_top <= max_y && area_bottom >= min_y)
        })
        .collect()
}

/// Get text alignment property
fn get_text_align(node_id: NodeId, styled_dom: &StyledDom) -> StyleTextAlign {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    css_property_cache
        .get_text_align(node_data, &node_id, styled_node_state)
        .and_then(|ta| ta.get_property().copied())
        .unwrap_or(StyleTextAlign::Left)
}

/// Extract text layout options from CSS properties
fn extract_text_layout_options(
    node_id: NodeId,
    styled_dom: &StyledDom,
) -> ResolvedTextLayoutOptions {
    use azul_core::ui_solver::DEFAULT_FONT_SIZE_PX;

    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    // Get font size
    let font_size = css_property_cache
        .get_font_size(node_data, &node_id, styled_node_state)
        .and_then(|fs| fs.get_property().copied())
        .map_or(DEFAULT_FONT_SIZE_PX as f32, |fs| {
            fs.inner.to_pixels(100.0 /* percent - TODO */)
        });

    // Get line height
    let line_height = css_property_cache
        .get_line_height(node_data, &node_id, styled_node_state)
        .and_then(|lh| Some(lh.get_property()?.inner.normalized()))
        .into();

    // Get letter spacing
    let letter_spacing = css_property_cache
        .get_letter_spacing(node_data, &node_id, styled_node_state)
        .and_then(|ls| Some(ls.get_property()?.inner.to_pixels(DEFAULT_LETTER_SPACING)))
        .into();

    // Get word spacing
    let word_spacing = css_property_cache
        .get_word_spacing(node_data, &node_id, styled_node_state)
        .and_then(|ws| Some(ws.get_property()?.inner.to_pixels(DEFAULT_WORD_SPACING)))
        .into();

    // Get tab width
    let tab_width = css_property_cache
        .get_tab_width(node_data, &node_id, styled_node_state)
        .and_then(|tw| Some(tw.get_property()?.inner.normalized()))
        .into();

    // Create and return ResolvedTextLayoutOptions
    ResolvedTextLayoutOptions {
        font_size_px: font_size,
        line_height,
        letter_spacing,
        word_spacing,
        tab_width,
        max_horizontal_width: None.into(),
        leading: None.into(),
        holes: Vec::new().into(),
        max_vertical_height: None.into(),
        can_break: true,
        can_hyphenate: false,
        hyphenation_character: None.into(),
        is_rtl: azul_core::ui_solver::ScriptType::LTR,
        text_justify: None.into(),
    }
}
