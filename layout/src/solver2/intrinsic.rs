use azul_core::{
    app_resources::{DecodedImage, RendererResources, RendererResourcesTrait},
    dom::NodeType,
    id_tree::{NodeDataContainer, NodeDataContainerRef},
    styled_dom::StyledDom,
    ui_solver::{FormattingContext, IntrinsicSizes},
    window::LogicalSize,
};
use azul_css::*;

use crate::parsedfont::ParsedFont;

/// Calculate the intrinsic sizes for all elements in the DOM
pub fn calculate_intrinsic_sizes<T: RendererResourcesTrait>(
    styled_dom: &StyledDom,
    formatting_contexts: &NodeDataContainer<FormattingContext>,
    renderer_resources: &T,
) -> NodeDataContainer<IntrinsicSizes> {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = styled_dom.node_data.as_container();
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();

    // Step 1: Calculate initial intrinsic sizes for leaf nodes
    let mut intrinsic_sizes = NodeDataContainer {
        internal: vec![IntrinsicSizes::default(); node_data.len()],
    };

    // Process leaf nodes first
    for node_id in node_data.linear_iter() {
        let node_data_ref = &node_data[node_id];
        match node_data_ref.get_node_type() {
            NodeType::Text(text) => {
                // Calculate text dimensions
                #[cfg(feature = "text_layout")]
                if let Some(text_sizes) = calculate_text_intrinsic_sizes(
                    node_id,
                    text.as_str(),
                    styled_dom,
                    renderer_resources,
                ) {
                    intrinsic_sizes.as_ref_mut()[node_id] = text_sizes;
                }
            }
            NodeType::Image(image) => {
                // Calculate image dimensions
                if let Some(image_sizes) =
                    calculate_image_intrinsic_sizes(image, styled_dom, renderer_resources)
                {
                    intrinsic_sizes.as_ref_mut()[node_id] = image_sizes;
                }
            }
            _ => {
                // Other nodes will get their size from children during the bubble-up phase
            }
        }
    }

    // Step 2: Bubble up sizes from children to parents (bottom-up traversal)
    // Start from the deepest nodes and work upward
    for depth_item in styled_dom.non_leaf_nodes.iter().rev() {
        if let Some(parent_id) = depth_item.node_id.into_crate_internal() {
            let formatting_context = &formatting_contexts.as_ref()[parent_id];
            let styled_node_state = &styled_nodes[parent_id].state;
            let node_data_ref = &node_data[parent_id];

            // Get parent's CSS size constraints
            let container_size = LogicalSize::new(800.0, 600.0); // Default viewport size assumption

            let width = css_property_cache.get_width(node_data_ref, &parent_id, styled_node_state);
            let min_width =
                css_property_cache.get_min_width(node_data_ref, &parent_id, styled_node_state);
            let max_width =
                css_property_cache.get_max_width(node_data_ref, &parent_id, styled_node_state);
            let height =
                css_property_cache.get_height(node_data_ref, &parent_id, styled_node_state);
            let min_height =
                css_property_cache.get_min_height(node_data_ref, &parent_id, styled_node_state);
            let max_height =
                css_property_cache.get_max_height(node_data_ref, &parent_id, styled_node_state);

            // Calculate parent's intrinsic size based on its children and formatting context
            let mut parent_sizes = calculate_parent_intrinsic_sizes(
                parent_id,
                formatting_context,
                styled_dom,
                &intrinsic_sizes.as_ref(),
            );

            // Apply CSS constraints to the calculated sizes
            parent_sizes.apply_constraints(
                width,
                min_width,
                max_width,
                height,
                min_height,
                max_height,
                container_size,
            );

            intrinsic_sizes.as_ref_mut()[parent_id] = parent_sizes;
        }
    }

    intrinsic_sizes
}

/// Calculate intrinsic sizes for text nodes
#[cfg(feature = "text_layout")]
fn calculate_text_intrinsic_sizes<T: RendererResourcesTrait>(
    node_id: azul_core::id_tree::NodeId,
    text: &str,
    styled_dom: &StyledDom,
    renderer_resources: &T,
) -> Option<IntrinsicSizes> {
    use azul_core::{styled_dom::StyleFontFamiliesHash, ui_solver::DEFAULT_FONT_SIZE_PX};

    use crate::text2::layout::{shape_words, split_text_into_words};

    let css_property_cache = styled_dom.get_css_property_cache();
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;
    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Get font information
    let font_size =
        css_property_cache.get_font_size_or_default(node_data, &node_id, styled_node_state);
    let font_size_px = font_size.inner.to_pixels(DEFAULT_FONT_SIZE_PX as f32);

    // Get font family and look up font in renderer resources
    let css_font_families =
        css_property_cache.get_font_id_or_default(node_data, &node_id, styled_node_state);

    let css_font_families_hash = StyleFontFamiliesHash::new(css_font_families.as_ref());
    let css_font_family = renderer_resources.get_font_family(&css_font_families_hash)?;
    let font_key = renderer_resources.get_font_key(&css_font_family)?;
    let (font_ref, _) = renderer_resources.get_registered_font(&font_key)?;

    // Get parsed font to access metrics and shaping functions
    let font_data = font_ref.get_data();
    let parsed_font = unsafe { &*(font_data.parsed as *const ParsedFont) };

    // Split text into words and calculate their dimensions
    let words = split_text_into_words(text);
    let shaped_words = shape_words(&words, parsed_font);

    // Calculate min content width (width of the longest word)
    let min_content_width = shaped_words.longest_word_width as f32 * font_size_px
        / parsed_font.font_metrics.units_per_em as f32;

    // Max content width (width if text doesn't wrap)
    let max_content_width = shaped_words
        .items
        .iter()
        .map(|word| word.word_width as f32)
        .sum::<f32>()
        * font_size_px
        / parsed_font.font_metrics.units_per_em as f32;

    // Calculate height based on line height
    let line_height = css_property_cache
        .get_line_height(node_data, &node_id, styled_node_state)
        .and_then(|lh| Some(lh.get_property()?.inner.normalized()))
        .unwrap_or(1.2); // Default line height multiplier

    let content_height = font_size_px * line_height;

    Some(IntrinsicSizes::new(
        min_content_width,
        max_content_width,
        Some(max_content_width), // Preferred width is max_content for text
        content_height,
        content_height,
        Some(content_height),
    ))
}

/// Calculate intrinsic sizes for image nodes
fn calculate_image_intrinsic_sizes<T: RendererResourcesTrait>(
    image: &azul_core::app_resources::ImageRef,
    styled_dom: &StyledDom,
    renderer_resources: &T,
) -> Option<IntrinsicSizes> {
    // Get image dimensions from the image data
    let (width, height) = match image.get_data() {
        DecodedImage::NullImage { width, height, .. } => ((*width) as f32, (*height) as f32),
        DecodedImage::Gl(tex) => (tex.size.width as f32, tex.size.height as f32),
        DecodedImage::Raw((desc, _)) => (desc.width as f32, desc.height as f32),
        _ => return None,
    };

    // For images, min and max content sizes are typically the same
    Some(IntrinsicSizes::new(
        width,
        width,
        Some(width),
        height,
        height,
        Some(height),
    ))
}

/// Calculate parent element's intrinsic sizes based on its children and formatting context
fn calculate_parent_intrinsic_sizes(
    parent_id: azul_core::id_tree::NodeId,
    formatting_context: &FormattingContext,
    styled_dom: &StyledDom,
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
) -> IntrinsicSizes {
    use azul_core::ui_solver::FormattingContext::*;

    let node_hierarchy = styled_dom.node_hierarchy.as_container();

    // Get children
    let children: Vec<_> = parent_id.az_children(&node_hierarchy).collect();

    // If no children, return empty sizes
    if children.is_empty() {
        return IntrinsicSizes::default();
    }

    match formatting_context {
        Block {
            establishes_new_context: _,
        } => calculate_block_intrinsic_sizes(&children, intrinsic_sizes),
        Inline => calculate_inline_intrinsic_sizes(&children, intrinsic_sizes),
        InlineBlock => calculate_inline_block_intrinsic_sizes(&children, intrinsic_sizes),
        Flex => calculate_flex_intrinsic_sizes(parent_id, &children, styled_dom, intrinsic_sizes),
        Float(_) => calculate_block_intrinsic_sizes(&children, intrinsic_sizes),
        OutOfFlow(_) => {
            // Out-of-flow elements (absolute/fixed) have same intrinsic sizing as blocks
            calculate_block_intrinsic_sizes(&children, intrinsic_sizes)
        }
        _ => {
            // TODO: other formatting contexts, table, etc.
            // Elements with display:none have no intrinsic size
            IntrinsicSizes::default()
        }
    }
}

/// Calculate intrinsic sizes for block formatting context
fn calculate_block_intrinsic_sizes(
    children: &[azul_core::id_tree::NodeId],
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
) -> IntrinsicSizes {
    // For block formatting context:
    // - Width: maximum of children's widths (children stack vertically, so width is determined by
    //   widest child)
    // - Height: sum of children's heights (children stack vertically)

    // Min content width is the maximum of children's min content widths
    let min_content_width = children
        .iter()
        .map(|&child_id| intrinsic_sizes[child_id].min_content_width)
        .fold(0.0, f32::max);

    // Max content width is the maximum of children's max content widths
    let max_content_width = children
        .iter()
        .map(|&child_id| intrinsic_sizes[child_id].max_content_width)
        .fold(0.0, f32::max);

    // Min content height is the sum of children's min content heights
    let min_content_height = children
        .iter()
        .map(|&child_id| intrinsic_sizes[child_id].min_content_height)
        .sum();

    // Max content height is the sum of children's max content heights
    let max_content_height = children
        .iter()
        .map(|&child_id| intrinsic_sizes[child_id].max_content_height)
        .sum();

    IntrinsicSizes::new(
        min_content_width,
        max_content_width,
        Some(max_content_width),
        min_content_height,
        max_content_height,
        Some(max_content_height),
    )
}

/// Calculate intrinsic sizes for inline formatting context
fn calculate_inline_intrinsic_sizes(
    children: &[azul_core::id_tree::NodeId],
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
) -> IntrinsicSizes {
    // For inline formatting context:
    // - Width: sum of all children (inline elements flow horizontally)
    // - Height: max height of children (line height depends on tallest inline element)

    // Min content width is the sum of children's min content widths
    // This assumes no line breaks - not accurate for real inline layout
    let min_content_width = children
        .iter()
        .map(|&child_id| intrinsic_sizes[child_id].min_content_width)
        .sum();

    // Max content width is the sum of children's max content widths
    let max_content_width = children
        .iter()
        .map(|&child_id| intrinsic_sizes[child_id].max_content_width)
        .sum();

    // Height is determined by the tallest inline element
    let content_height = children
        .iter()
        .map(|&child_id| intrinsic_sizes[child_id].max_content_height)
        .fold(0.0, f32::max);

    IntrinsicSizes::new(
        min_content_width,
        max_content_width,
        Some(max_content_width),
        content_height,
        content_height,
        Some(content_height),
    )
}

/// Calculate intrinsic sizes for inline-block formatting context
fn calculate_inline_block_intrinsic_sizes(
    children: &[azul_core::id_tree::NodeId],
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
) -> IntrinsicSizes {
    // Inline-block creates a block formatting context but participates in inline layout
    // Its size calculation is similar to block elements

    // Min content width is the maximum of children's min content widths
    let min_content_width = children
        .iter()
        .map(|&child_id| intrinsic_sizes[child_id].min_content_width)
        .fold(0.0, f32::max);

    // Max content width is the maximum of children's max content widths
    let max_content_width = children
        .iter()
        .map(|&child_id| intrinsic_sizes[child_id].max_content_width)
        .fold(0.0, f32::max);

    // Min content height is the sum of children's min content heights
    let min_content_height = children
        .iter()
        .map(|&child_id| intrinsic_sizes[child_id].min_content_height)
        .sum();

    // Max content height is the sum of children's max content heights
    let max_content_height = children
        .iter()
        .map(|&child_id| intrinsic_sizes[child_id].max_content_height)
        .sum();

    IntrinsicSizes::new(
        min_content_width,
        max_content_width,
        Some(max_content_width),
        min_content_height,
        max_content_height,
        Some(max_content_height),
    )
}

/// Calculate intrinsic sizes for flex formatting context
fn calculate_flex_intrinsic_sizes(
    parent_id: azul_core::id_tree::NodeId,
    children: &[azul_core::id_tree::NodeId],
    styled_dom: &StyledDom,
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
) -> IntrinsicSizes {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = styled_dom.node_data.as_container();
    let styled_node_state = &styled_dom.styled_nodes.as_container()[parent_id].state;

    // Get flex direction
    let flex_direction = css_property_cache
        .get_flex_direction(&node_data[parent_id], &parent_id, styled_node_state)
        .and_then(|dir| dir.get_property().copied())
        .unwrap_or_default();

    let is_row = flex_direction.get_axis() == LayoutAxis::Horizontal;

    if is_row {
        // For row direction, width is sum of children, height is max of children

        // Min content width is sum of children's min content widths
        let min_content_width = children
            .iter()
            .map(|&child_id| intrinsic_sizes[child_id].min_content_width)
            .sum();

        // Max content width is sum of children's max content widths
        let max_content_width = children
            .iter()
            .map(|&child_id| intrinsic_sizes[child_id].max_content_width)
            .sum();

        // Min/max content height is max of children's heights
        let min_content_height = children
            .iter()
            .map(|&child_id| intrinsic_sizes[child_id].min_content_height)
            .fold(0.0, f32::max);

        let max_content_height = children
            .iter()
            .map(|&child_id| intrinsic_sizes[child_id].max_content_height)
            .fold(0.0, f32::max);

        IntrinsicSizes::new(
            min_content_width,
            max_content_width,
            Some(max_content_width),
            min_content_height,
            max_content_height,
            Some(max_content_height),
        )
    } else {
        // For column direction, width is max of children, height is sum of children

        // Min/max content width is max of children's widths
        let min_content_width = children
            .iter()
            .map(|&child_id| intrinsic_sizes[child_id].min_content_width)
            .fold(0.0, f32::max);

        let max_content_width = children
            .iter()
            .map(|&child_id| intrinsic_sizes[child_id].max_content_width)
            .fold(0.0, f32::max);

        // Min content height is sum of children's min content heights
        let min_content_height = children
            .iter()
            .map(|&child_id| intrinsic_sizes[child_id].min_content_height)
            .sum();

        // Max content height is sum of children's max content heights
        let max_content_height = children
            .iter()
            .map(|&child_id| intrinsic_sizes[child_id].max_content_height)
            .sum();

        IntrinsicSizes::new(
            min_content_width,
            max_content_width,
            Some(max_content_width),
            min_content_height,
            max_content_height,
            Some(max_content_height),
        )
    }
}
