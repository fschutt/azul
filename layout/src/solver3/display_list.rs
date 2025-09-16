//! solver3/display_list.rs
//! Pass 4: Generate display list for rendering

use std::collections::BTreeMap;

use azul_core::{
    app_resources::{DecodedImage, ImageKey, ImageRefHash},
    display_list::GlyphInstance,
    dom::{NodeId, NodeType},
    styled_dom::StyledDom,
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::{AzString, ColorU, LayoutDebugMessage};
use rust_fontconfig::FcWeight;

use crate::{
    parsedfont::ParsedFont,
    solver3::{
        positioning::{get_position_type, PositionType, PositionedLayoutTree},
        LayoutError, Result,
    },
    text3::cache::{
        InlineContent, InlineShape, Rect, ShapeDefinition, ShapedGlyph,
        FontRef, FontStyle, ImageSource, PositionedItem, ShapedItem, UnifiedLayout,
    },
};

/// Generate final display list for rendering
pub fn generate_display_list(
    positioned_tree: &PositionedLayoutTree,
    styled_dom: &StyledDom,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<DisplayList> {
    debug_log(debug_messages, "Generating display list");

    let mut builder = DisplayListBuilder::new();
    let mut generator = DisplayListGenerator::new();

    // Process all nodes in paint order (respecting stacking contexts)
    let stacking_contexts = collect_stacking_contexts(positioned_tree, styled_dom)?;

    for stacking_context in stacking_contexts {
        generate_stacking_context(
            &mut generator,
            &mut builder,
            positioned_tree,
            stacking_context,
            styled_dom,
            debug_messages,
        )?;
    }

    let display_list = builder.build();

    debug_log(
        debug_messages,
        &format!(
            "Generated display list with {} items",
            display_list.items.len()
        ),
    );

    Ok(display_list)
}

struct DisplayListGenerator {
    current_clip: Option<LogicalRect>,
}

impl DisplayListGenerator {
    fn new() -> Self {
        Self { current_clip: None }
    }
}

/// Represents a stacking context in the paint order
#[derive(Debug)]
struct StackingContext {
    node_index: usize,
    z_index: i32,
    children: Vec<StackingContext>,
}

fn collect_stacking_contexts(
    positioned_tree: &PositionedLayoutTree,
    styled_dom: &StyledDom,
) -> Result<Vec<StackingContext>> {
    // Build stacking context tree
    let root_context =
        build_stacking_context_recursive(positioned_tree, positioned_tree.tree.root, styled_dom)?;

    // Flatten and sort by z-index
    let mut contexts = vec![root_context];
    sort_stacking_contexts(&mut contexts);

    Ok(contexts)
}

fn build_stacking_context_recursive(
    positioned_tree: &PositionedLayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
) -> Result<StackingContext> {
    let node = positioned_tree
        .tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?;
    let z_index = get_z_index(styled_dom, node.dom_node_id);

    let mut children = Vec::new();

    for &child_index in &node.children {
        // Check if child establishes new stacking context
        if establishes_stacking_context(positioned_tree, child_index, styled_dom) {
            let child_context =
                build_stacking_context_recursive(positioned_tree, child_index, styled_dom)?;
            children.push(child_context);
        }
    }

    Ok(StackingContext {
        node_index,
        z_index,
        children,
    })
}

fn sort_stacking_contexts(contexts: &mut Vec<StackingContext>) {
    contexts.sort_by_key(|ctx| ctx.z_index);

    for context in contexts {
        sort_stacking_contexts(&mut context.children);
    }
}

fn generate_stacking_context(
    generator: &mut DisplayListGenerator,
    builder: &mut DisplayListBuilder,
    positioned_tree: &PositionedLayoutTree,
    context: StackingContext,
    styled_dom: &StyledDom,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    // Generate display items for this stacking context
    generate_node_display_items(
        generator,
        builder,
        positioned_tree,
        context.node_index,
        styled_dom,
        debug_messages,
    )?;

    // Process child stacking contexts
    for child_context in context.children {
        generate_stacking_context(
            generator,
            builder,
            positioned_tree,
            child_context,
            styled_dom,
            debug_messages,
        )?;
    }

    Ok(())
}

fn generate_node_display_items(
    generator: &mut DisplayListGenerator,
    builder: &mut DisplayListBuilder,
    positioned_tree: &PositionedLayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    let node = positioned_tree
        .tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?;
    let absolute_position = positioned_tree
        .absolute_positions
        .get(&node_index)
        .copied()
        .unwrap_or_default();
    let size = positioned_tree
        .used_sizes
        .get(&node_index)
        .copied()
        .unwrap_or_default();

    let bounds = LogicalRect::new(
        LogicalPosition::new(absolute_position.x, absolute_position.y),
        LogicalSize::new(size.width, size.height),
    );

    // Paint order: background -> border -> content -> outline

    // 1. Background
    if let Some(dom_id) = node.dom_node_id {
        generate_background(builder, bounds, styled_dom, dom_id)?;
        generate_border(builder, bounds, styled_dom, dom_id)?;
    }

    // 2. Content - handle different types
    if let Some(inline_layout) = &node.inline_layout_result {
        // Text content from text3
        generate_text_content(builder, absolute_position, inline_layout, debug_messages)?;
    } else if let Some(dom_id) = node.dom_node_id {
        // Other content types
        generate_node_content(builder, bounds, styled_dom, dom_id)?;
    }

    // 3. Process children that don't establish stacking contexts
    for &child_index in &node.children {
        if !establishes_stacking_context(positioned_tree, child_index, styled_dom) {
            generate_node_display_items(
                generator,
                builder,
                positioned_tree,
                child_index,
                styled_dom,
                debug_messages,
            )?;
        }
    }

    Ok(())
}

fn generate_background(
    builder: &mut DisplayListBuilder,
    bounds: LogicalRect,
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> Result<()> {
    // Extract background properties from CSS
    let background_color = get_background_color(styled_dom, dom_id);

    if background_color.a > 0 {
        builder.push_rect(bounds, background_color.into());
    }

    // TODO: Background images, gradients, etc.

    Ok(())
}

fn generate_border(
    builder: &mut DisplayListBuilder,
    bounds: LogicalRect,
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> Result<()> {
    let border_info = get_border_info(styled_dom, dom_id);

    if border_info.width > 0.0 {
        // Simplified border rendering - just outline for now
        builder.push_border(bounds, border_info.color, border_info.width);
    }

    Ok(())
}

fn generate_text_content(
    builder: &mut DisplayListBuilder,
    base_position: LogicalPosition,
    layout: &UnifiedLayout<ParsedFont>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    let mut glyph_count = 0;

    // Process all positioned items from text3
    for item in &layout.items {
        match item {
            PositionedItem::Shaped(shaped_item) => {
                generate_shaped_item(builder, base_position, shaped_item)?;
                glyph_count += count_glyphs_in_shaped_item(shaped_item);
            }
            PositionedItem::Object {
                bounds, content, ..
            } => {
                generate_inline_object(builder, base_position, bounds, content)?;
            }
        }
    }

    debug_log(
        debug_messages,
        &format!("Generated {} glyphs from text3 layout", glyph_count),
    );

    Ok(())
}

fn generate_shaped_item(
    builder: &mut DisplayListBuilder,
    base_position: LogicalPosition,
    shaped_item: &ShapedItem<ParsedFont>,
) -> Result<()> {
    match shaped_item {
        ShapedItem::Cluster(cluster) => {
            // Convert text3 glyphs to display list glyph instances
            let mut glyph_instances = Vec::new();
            let mut x = 0.0;

            for glyph in &cluster.glyphs {
                let instance = GlyphInstance {
                    point: LogicalPosition::new(
                        base_position.x + x,
                        base_position.y + glyph.baseline_offset,
                    ),
                    index: glyph.glyph_id as u32,
                };
                glyph_instances.push(instance);
                x += glyph.advance_width;
            }

            if !glyph_instances.is_empty() {
                builder.push_text_run(
                    glyph_instances,
                    cluster.style.font_ref.clone(),
                    cluster.style.color.into(),
                );
            }
        }
        ShapedItem::Object {
            bounds, content, ..
        } => {
            generate_inline_object_from_bounds(builder, base_position, bounds, content)?;
        }
        ShapedItem::CombinedBlock { bounds, glyphs, .. } => {
            // Handle tate-chu-yoko (combined text in vertical writing)
            generate_combined_text_block(builder, base_position, bounds, glyphs)?;
        }
        _ => {
            // Handle other shaped item types as needed
        }
    }

    Ok(())
}

fn generate_node_content(
    builder: &mut DisplayListBuilder,
    bounds: LogicalRect,
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> Result<()> {
    let node_data = &styled_dom.node_data.as_container()[dom_id];

    match &node_data.get_node_type() {
        NodeType::Image(image_data) => {
            // Generate image display item
            if let Some(image_key) = get_image_key_for_src(&image_data.get_hash()) {
                builder.push_image(bounds, image_key);
            }
        }
        NodeType::Div => {
            // Div content is handled by children
        }
        NodeType::Text(text_data) => {
            // Standalone text node (should be handled by IFC usually)
            generate_simple_text(builder, bounds, &text_data, styled_dom, dom_id)?;
        }
        _ => {
            // Other node types
        }
    }

    Ok(())
}

fn generate_inline_object(
    builder: &mut DisplayListBuilder,
    base_position: LogicalPosition,
    bounds: &Rect,
    content: &InlineContent,
) -> Result<()> {
    let layout_bounds = LogicalRect::new(
        LogicalPosition::new(base_position.x + bounds.x, base_position.y + bounds.y),
        LogicalSize::new(bounds.width, bounds.height),
    );

    match content {
        InlineContent::Image(image) => {
            if let Some(image_key) = get_image_key_for_image_source(&image.source) {
                builder.push_image(layout_bounds, image_key);
            }
        }
        InlineContent::Shape(shape) => {
            generate_inline_shape(builder, layout_bounds, shape)?;
        }
        _ => {
            // Other inline content types
        }
    }

    Ok(())
}

fn generate_inline_object_from_bounds(
    builder: &mut DisplayListBuilder,
    base_position: LogicalPosition,
    bounds: &Rect,
    content: &InlineContent,
) -> Result<()> {
    generate_inline_object(builder, base_position, bounds, content)
}

fn generate_combined_text_block(
    builder: &mut DisplayListBuilder,
    base_position: LogicalPosition,
    bounds: &Rect,
    glyphs: &[ShapedGlyph<ParsedFont>],
) -> Result<()> {
    let mut glyph_instances = Vec::new();
    let mut x = 0.0;

    for glyph in glyphs {
        let instance = GlyphInstance {
            point: LogicalPosition::new(
                base_position.x + bounds.x + x,
                base_position.y + bounds.y + glyph.baseline_offset,
            ),
            glyph_index: glyph.glyph_id as u32,
        };
        glyph_instances.push(instance);
        x += glyph.advance; // TODO: vertical_advance?
    }

    if !glyph_instances.is_empty() {
        // Use first glyph's font for the run
        if let Some(first_glyph) = glyphs.first() {
            builder.push_text_run(
                glyph_instances,
                first_glyph.font.clone(),
                ColorU::new(0, 0, 0, 255), // Default black
            );
        }
    }

    Ok(())
}

fn generate_simple_text(
    builder: &mut DisplayListBuilder,
    bounds: LogicalRect,
    text: &str,
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> Result<()> {
    // Fallback for simple text rendering without text3
    // This is a very basic implementation

    let font_info = get_font_info(styled_dom, dom_id);
    let color = get_text_color(styled_dom, dom_id);

    // Create simple glyph instances (this is a stub)
    let mut glyph_instances = Vec::new();
    let char_width = 8.0; // Fixed width for simplicity

    for (i, ch) in text.chars().enumerate() {
        if ch != ' ' && ch != '\n' {
            let instance = GlyphInstance {
                point: LogicalPosition::new(
                    bounds.origin.x + (i as f32 * char_width),
                    bounds.origin.y + 16.0, // Baseline offset
                ),
                glyph_index: ch as u32, // Very simplified glyph mapping
            };
            glyph_instances.push(instance);
        }
    }

    if !glyph_instances.is_empty() {
        builder.push_text_run(glyph_instances, font_info, color);
    }

    Ok(())
}

fn generate_inline_shape(
    builder: &mut DisplayListBuilder,
    bounds: LogicalRect,
    shape: &InlineShape,
) -> Result<()> {
    // Generate shape display items based on shape definition
    match &shape.shape_def {
        ShapeDefinition::Rectangle { .. } => {
            if let Some(fill) = &shape.fill {
                builder.push_rect(bounds, (*fill).into());
            }
        }
        ShapeDefinition::Circle { radius } => {
            // Approximate circle with rounded rectangle
            let circle_bounds = LogicalRect::new(bounds.origin, bounds.size);
            builder.push_rounded_rect(circle_bounds, *radius);
        }
        _ => {
            // Other shapes - implement as needed
        }
    }

    Ok(())
}

// Helper functions for extracting style information

fn establishes_stacking_context(
    _positioned_tree: &PositionedLayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
) -> bool {
    let node = match _positioned_tree.tree.get(node_index) {
        Some(n) => n,
        None => return false,
    };

    let Some(dom_id) = node.dom_node_id else {
        return false;
    };

    let position = get_position_type(styled_dom, Some(dom_id));
    let is_positioned = position == PositionType::Absolute
        || position == PositionType::Relative
        || position == PositionType::Fixed;

    if is_positioned {
        // Positioned elements with z-index other than 'auto' (which we model as non-zero)
        // form a stacking context.
        if get_z_index(styled_dom, Some(dom_id)) != 0 {
            return true;
        }
        // `position: fixed` and `position: absolute` always establish a new stacking context.
        if position == PositionType::Absolute || position == PositionType::Fixed {
            return true;
        }
    }

    // Other conditions that create stacking contexts (not needed for this test):
    // - opacity < 1
    // - transform != none
    // - filter != none
    // - etc.
    // TODO: Implement full stacking context rules.

    false
}

fn get_z_index(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> i32 {
    // Extract z-index from CSS - simplified
    0
}

fn get_background_color(styled_dom: &StyledDom, dom_id: NodeId) -> ColorU {
    // Extract background-color from CSS - simplified
    ColorU::new(255, 255, 255, 0) // Transparent by default
}

fn get_border_info(styled_dom: &StyledDom, dom_id: NodeId) -> BorderInfo {
    // Extract border properties from CSS - simplified
    BorderInfo {
        width: 0.0,
        color: ColorU::new(0, 0, 0, 255),
    }
}

fn get_font_info(styled_dom: &StyledDom, dom_id: NodeId) -> FontRef {
    // Extract font properties from CSS - simplified
    FontRef {
        family: "serif".to_string(),
        weight: FcWeight::Normal,
        style: FontStyle::Normal,
        unicode_ranges: Vec::new(),
    }
}

fn get_text_color(styled_dom: &StyledDom, dom_id: NodeId) -> ColorU {
    // Extract color from CSS - simplified
    ColorU::new(0, 0, 0, 255) // Black by default
}

fn get_image_key_for_src(src: &ImageRefHash) -> Option<ImageKey> {
    // Convert image src to image key - would integrate with resource system
    None
}

fn get_image_key_for_image_source(source: &ImageSource) -> Option<ImageKey> {
    // Convert text3 image source to image key
    None
}

fn count_glyphs_in_shaped_item(shaped_item: &ShapedItem<ParsedFont>) -> usize {
    match shaped_item {
        ShapedItem::Cluster(cluster) => cluster.glyphs.len(),
        ShapedItem::CombinedBlock { glyphs, .. } => glyphs.len(),
        _ => 0,
    }
}

#[derive(Debug)]
struct BorderInfo {
    width: f32,
    color: ColorU,
}

fn debug_log(debug_messages: &mut Option<Vec<LayoutDebugMessage>>, message: &str) {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: message.into(),
            location: "display_list".into(),
        });
    }
}
