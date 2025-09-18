//! solver3/display_list.rs
//! Pass 4: Generate display list for rendering

use std::collections::BTreeMap;

use azul_core::{
    app_resources::{DecodedImage, ImageKey, ImageRefHash},
    callbacks::ScrollPosition,
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
        LayoutContext, LayoutError, Result,
    },
    text3::cache::{
        FontLoaderTrait, FontRef, FontStyle, ImageSource, InlineContent, InlineShape,
        ParsedFontTrait, PositionedItem, Rect, ShapeDefinition, ShapedGlyph, ShapedItem,
        UnifiedLayout,
    },
};

/// Generate final display list for rendering
pub fn generate_display_list<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    positioned_tree: &PositionedLayoutTree,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
) -> Result<DisplayList> {
    ctx.debug_log("Generating display list");

    let mut builder = DisplayListBuilder::new();
    let mut generator = DisplayListGenerator::new(scroll_offsets);

    // 1. Build a tree of stacking contexts based on the layout tree.
    let stacking_context_tree =
        collect_stacking_contexts(ctx, positioned_tree, positioned_tree.tree.root)?;

    // 2. The root context is the base; traverse it to generate all display items.
    generate_for_stacking_context(
        &mut generator,
        &mut builder,
        positioned_tree,
        &stacking_context_tree,
        &ctx.styled_dom,
        ctx.debug_messages,
    )?;

    let display_list = builder.build();

    ctx.debug_log(&format!(
        "Generated display list with {} items",
        display_list.items.len()
    ));

    Ok(display_list)
}

struct DisplayListGenerator<'a> {
    current_clip: Option<LogicalRect>,
    scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>,
}

impl<'a> DisplayListGenerator<'a> {
    pub fn new(scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>) -> Self {
        Self {
            current_clip: None,
            scroll_offsets,
        }
    }
}

/// Represents a stacking context in the paint order. This is a tree structure.
#[derive(Debug)]
pub struct StackingContext {
    /// The layout node that establishes this context.
    pub node_index: usize,
    /// The z-index of this context.
    pub z_index: i32,
    /// Children that are *not* stacking contexts, painted in DOM order.
    pub in_flow_children: Vec<usize>,
    /// Children that establish their own stacking contexts.
    pub child_contexts: Vec<StackingContext>,
}

/// Recursively builds the tree of stacking contexts.
pub fn collect_stacking_contexts<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    positioned_tree: &PositionedLayoutTree,
    node_index: usize,
) -> Result<StackingContext> {
    let node = positioned_tree
        .tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?;
    let z_index = get_z_index(ctx.styled_dom, node.dom_node_id);

    let mut in_flow_children = Vec::new();
    let mut child_contexts = Vec::new();

    for &child_index in &node.children {
        if establishes_stacking_context(ctx, positioned_tree, child_index) {
            let child_context = collect_stacking_contexts(ctx, positioned_tree, child_index)?;
            child_contexts.push(child_context);
        } else {
            in_flow_children.push(child_index);
        }
    }

    Ok(StackingContext {
        node_index,
        z_index,
        in_flow_children,
        child_contexts,
    })
}

/// Main recursive function to generate display items based on the stacking context tree.
pub fn generate_for_stacking_context(
    generator: &mut DisplayListGenerator,
    builder: &mut DisplayListBuilder,
    positioned_tree: &PositionedLayoutTree,
    context: &StackingContext,
    styled_dom: &StyledDom,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    // CSS Paint Order Spec (simplified):
    // 1. Background and borders for the context's root element.
    paint_node_background_and_border(
        builder,
        generator,
        builder,
        positioned_tree,
        context.node_index,
    )?;

    // 2. Child stacking contexts with negative z-indices.
    let mut negative_z_children: Vec<_> = context
        .child_contexts
        .iter()
        .filter(|c| c.z_index < 0)
        .collect();
    negative_z_children.sort_by_key(|c| c.z_index);
    for child in negative_z_children {
        generate_for_stacking_context(
            generator,
            builder,
            positioned_tree,
            child,
            styled_dom,
            debug_messages,
        )?;
    }

    // 3. In-flow, non-positioned descendants.
    paint_in_flow_descendants(
        generator,
        builder,
        positioned_tree,
        context.node_index,
        context,
        styled_dom,
        debug_messages,
    )?;

    // 4. Child stacking contexts with z-index: 0 / auto.
    for child in context.child_contexts.iter().filter(|c| c.z_index == 0) {
        generate_for_stacking_context(
            generator,
            builder,
            positioned_tree,
            child,
            styled_dom,
            debug_messages,
        )?;
    }

    // 5. Child stacking contexts with positive z-indices.
    let mut positive_z_children: Vec<_> = context
        .child_contexts
        .iter()
        .filter(|c| c.z_index > 0)
        .collect();
    positive_z_children.sort_by_key(|c| c.z_index);
    for child in positive_z_children {
        generate_for_stacking_context(
            generator,
            builder,
            positioned_tree,
            child,
            styled_dom,
            debug_messages,
        )?;
    }

    Ok(())
}

/// Paints the content and non-stacking-context children of a given node.
pub fn paint_in_flow_descendants(
    generator: &mut DisplayListGenerator,
    builder: &mut DisplayListBuilder,
    positioned_tree: &PositionedLayoutTree,
    node_index: usize,
    context: &StackingContext, // Pass context to get correct child list
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
    let bounds = LogicalRect::new(absolute_position, size);

    // 1. Paint the node's own content (text, images).
    if let Some(inline_layout) = &node.inline_layout_result {
        generate_text_content(builder, absolute_position, inline_layout, debug_messages)?;
    } else if let Some(dom_id) = node.dom_node_id {
        generate_node_content(builder, bounds, styled_dom, dom_id)?;
    }

    // 2. Recursively paint the in-flow children.
    // Use the `in_flow_children` from the context for the root of this sub-paint.
    // For deeper children, use their own children list.
    let children_to_paint = if node_index == context.node_index {
        &context.in_flow_children
    } else {
        &node.children
    };

    for &child_index in children_to_paint {
        // Since these children do not form stacking contexts, we paint their
        // background/border first, then recurse.
        paint_node_background_and_border(
            builder,
            generator,
            builder,
            positioned_tree,
            child_index,
        )?;
        paint_in_flow_descendants(
            generator,
            builder,
            positioned_tree,
            child_index,
            context,
            styled_dom,
            debug_messages,
        )?;
    }
    Ok(())
}

/// Helper to paint just the background and border of a single node.
pub fn paint_node_background_and_border<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    generator: &mut DisplayListGenerator,
    builder: &mut DisplayListBuilder,
    positioned_tree: &PositionedLayoutTree,
    node_index: usize,
) -> Result<()> {
    let node = positioned_tree
        .tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?;
    let Some(dom_id) = node.dom_node_id else {
        return Ok(());
    };

    let mut absolute_position = positioned_tree
        .absolute_positions
        .get(&node_index)
        .copied()
        .unwrap_or_default();
    let size = positioned_tree
        .used_sizes
        .get(&node_index)
        .copied()
        .unwrap_or_default();

    // Apply scroll offset from parent for painting
    if let Some(parent_idx) = node.parent {
        if let Some(parent_dom_id) = positioned_tree
            .tree
            .get(parent_idx)
            .and_then(|p| p.dom_node_id)
        {
            if let Some(scroll_offset) = generator.scroll_offsets.get(&parent_dom_id) {
                absolute_position.x -= scroll_offset.x;
                absolute_position.y -= scroll_offset.y;
            }
        }
    }

    let bounds = LogicalRect::new(absolute_position, size);

    generate_background(builder, bounds, ctx.styled_dom, dom_id)?;
    generate_border(builder, bounds, ctx.styled_dom, dom_id)?;
    Ok(())
}

pub fn generate_combined_text_block(
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
            // Corrected field name from `glyph_index` to `index` for consistency
            index: glyph.glyph_id as u32,
        };
        glyph_instances.push(instance);
        x += glyph.advance; // For tate-chu-yoko, `advance` is correct. Vertical text uses
                            // `vertical_advance`.
    }

    if !glyph_instances.is_empty() {
        if let Some(first_glyph) = glyphs.first() {
            builder.push_text_run(
                glyph_instances,
                first_glyph.font.clone(),
                ColorU::new(0, 0, 0, 255), // TODO: Get color from style
            );
        }
    }

    Ok(())
}

pub fn generate_simple_text(
    builder: &mut DisplayListBuilder,
    bounds: LogicalRect,
    text: &str,
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> Result<()> {
    let font_info = get_font_info(styled_dom, dom_id);
    let color = get_text_color(styled_dom, dom_id);
    let mut glyph_instances = Vec::new();
    let char_width = 8.0;

    for (i, ch) in text.chars().enumerate() {
        if ch.is_whitespace() {
            continue;
        }
        let instance = GlyphInstance {
            point: LogicalPosition::new(
                bounds.origin.x + (i as f32 * char_width),
                bounds.origin.y + 16.0,
            ),
            // Corrected field name from `glyph_index` to `index`
            index: ch as u32,
        };
        glyph_instances.push(instance);
    }

    if !glyph_instances.is_empty() {
        builder.push_text_run(glyph_instances, font_info, color);
    }

    Ok(())
}

pub fn generate_background(
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

pub fn generate_border(
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

pub fn generate_text_content(
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

pub fn generate_shaped_item(
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
            generate_inline_object(builder, base_position, bounds, content)?;
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

pub fn generate_node_content(
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

pub fn generate_inline_object(
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

pub fn generate_inline_shape(
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

pub fn establishes_stacking_context<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    positioned_tree: &PositionedLayoutTree,
    node_index: usize,
) -> bool {
    let node = match positioned_tree.tree.get(node_index) {
        Some(n) => n,
        None => return false,
    };

    let Some(dom_id) = node.dom_node_id else {
        return false;
    };

    let position = get_position_type(ctx.styled_dom, Some(dom_id));
    let is_positioned = position == PositionType::Absolute
        || position == PositionType::Relative
        || position == PositionType::Fixed;

    if is_positioned {
        if get_z_index(ctx.styled_dom, Some(dom_id)) != 0 {
            return true;
        }
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

pub fn get_z_index(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> i32 {
    // Extract z-index from CSS - simplified
    0
}

pub fn get_background_color(styled_dom: &StyledDom, dom_id: NodeId) -> ColorU {
    // Extract background-color from CSS - simplified
    ColorU::new(255, 255, 255, 0) // Transparent by default
}

pub fn get_border_info(styled_dom: &StyledDom, dom_id: NodeId) -> BorderInfo {
    // Extract border properties from CSS - simplified
    BorderInfo {
        width: 0.0,
        color: ColorU::new(0, 0, 0, 255),
    }
}

pub fn get_font_info(styled_dom: &StyledDom, dom_id: NodeId) -> FontRef {
    // Extract font properties from CSS - simplified
    FontRef {
        family: "serif".to_string(),
        weight: FcWeight::Normal,
        style: FontStyle::Normal,
        unicode_ranges: Vec::new(),
    }
}

pub fn get_text_color(styled_dom: &StyledDom, dom_id: NodeId) -> ColorU {
    // Extract color from CSS - simplified
    ColorU::new(0, 0, 0, 255) // Black by default
}

pub fn get_image_key_for_src(src: &ImageRefHash) -> Option<ImageKey> {
    // Convert image src to image key - would integrate with resource system
    None
}

pub fn get_image_key_for_image_source(source: &ImageSource) -> Option<ImageKey> {
    // Convert text3 image source to image key
    None
}

pub fn count_glyphs_in_shaped_item(shaped_item: &ShapedItem<ParsedFont>) -> usize {
    match shaped_item {
        ShapedItem::Cluster(cluster) => cluster.glyphs.len(),
        ShapedItem::CombinedBlock { glyphs, .. } => glyphs.len(),
        _ => 0,
    }
}

#[derive(Debug)]
pub struct BorderInfo {
    pub width: f32,
    pub color: ColorU,
}

pub fn debug_log(debug_messages: &mut Option<Vec<LayoutDebugMessage>>, message: &str) {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: message.into(),
            location: "display_list".into(),
        });
    }
}
