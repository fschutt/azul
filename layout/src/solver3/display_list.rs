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
        layout_tree::{LayoutNode, LayoutTree},
        positioning::{get_position_type, PositionType},
        LayoutContext, LayoutError, Result,
    },
    text3::cache::{
        FontLoaderTrait, FontRef, FontStyle, ImageSource, InlineContent, InlineShape,
        ParsedFontTrait, PositionedItem, Rect, ShapeDefinition, ShapedGlyph, ShapedItem,
        UnifiedLayout,
    },
};

// Helper struct to pass around layout results for painting
pub struct PositionedTree<'a, T: ParsedFontTrait> {
    pub tree: &'a LayoutTree<T>,
    pub absolute_positions: &'a BTreeMap<usize, LogicalPosition>,
}

#[derive(Debug, Default)]
pub struct DisplayList {
    pub items: Vec<DisplayListItem>,
}

#[derive(Debug, Default)]
pub struct DisplayListBuilder {
    items: Vec<DisplayListItem>,
}

impl DisplayListBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn build(self) -> DisplayList {
        DisplayList { items: self.items }
    }
    pub fn push_rect(&mut self, bounds: LogicalRect, color: ColorU) {
        self.items.push(DisplayListItem::Rect { bounds, color });
    }
    pub fn push_rounded_rect(&mut self, bounds: LogicalRect, _radius: f32, color: ColorU) {
        self.items.push(DisplayListItem::Rect { bounds, color });
    }
    pub fn push_border(&mut self, bounds: LogicalRect, color: ColorU, width: f32) {
        self.items.push(DisplayListItem::Border {
            bounds,
            color,
            width,
        });
    }
    pub fn push_text_run(&mut self, glyphs: Vec<GlyphInstance>, font: FontRef, color: ColorU) {
        self.items.push(DisplayListItem::Text {
            glyphs,
            font,
            color,
        });
    }
    pub fn push_image(&mut self, bounds: LogicalRect, key: ImageKey) {
        self.items.push(DisplayListItem::Image { bounds, key });
    }
}

/// Generate final display list for rendering
pub fn generate_display_list<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    tree: &LayoutTree<T>,
    absolute_positions: &BTreeMap<usize, LogicalPosition>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
) -> Result<DisplayList> {
    ctx.debug_log("Generating display list");
    let positioned_tree = PositionedTree {
        tree,
        absolute_positions,
    };
    let mut builder = DisplayListBuilder::new();
    let mut generator = DisplayListGenerator::new(scroll_offsets);

    // 1. Build a tree of stacking contexts based on the layout tree.
    let stacking_context_tree =
        collect_stacking_contexts(ctx, &positioned_tree, positioned_tree.tree.root)?;

    // 2. The root context is the base; traverse it to generate all display items.
    generate_for_stacking_context(
        &mut generator,
        &mut builder,
        &positioned_tree,
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
    scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>,
}

impl<'a> DisplayListGenerator<'a> {
    pub fn new(scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>) -> Self {
        Self { scroll_offsets }
    }
}

#[derive(Debug)]
pub struct StackingContext {
    pub node_index: usize,
    pub z_index: i32,
    pub in_flow_children: Vec<usize>,
    pub child_contexts: Vec<StackingContext>,
}

/// Recursively builds the tree of stacking contexts.
pub fn collect_stacking_contexts<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    positioned_tree: &PositionedTree<T>,
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
pub fn generate_for_stacking_context<T: ParsedFontTrait>(
    generator: &mut DisplayListGenerator,
    builder: &mut DisplayListBuilder,
    positioned_tree: &PositionedTree<T>,
    context: &StackingContext,
    styled_dom: &StyledDom,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    // CSS Paint Order Spec (simplified):
    // 1. Background and borders for the context's root element.
    paint_node_background_and_border(generator, builder, positioned_tree, context.node_index)?;

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
pub fn paint_in_flow_descendants<T: ParsedFontTrait>(
    generator: &mut DisplayListGenerator,
    builder: &mut DisplayListBuilder,
    positioned_tree: &PositionedTree<T>,
    node_index: usize,
    context: &StackingContext,
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
    let size = node.used_size.unwrap_or_default();
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
        paint_node_background_and_border(generator, builder, positioned_tree, child_index)?;
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
pub fn paint_node_background_and_border<T: ParsedFontTrait>(
    generator: &mut DisplayListGenerator,
    builder: &mut DisplayListBuilder,
    positioned_tree: &PositionedTree<T>,
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
    let size = node.used_size.unwrap_or_default();

    // Apply scroll offset from parent for painting
    if let Some(parent_idx) = node.parent {
        if let Some(parent_dom_id) = positioned_tree
            .tree
            .get(parent_idx)
            .and_then(|p| p.dom_node_id)
        {
            if let Some(scroll_offset) = generator.scroll_offsets.get(&parent_dom_id) {
                absolute_position.x -= scroll_offset.children_rect.origin.x;
                absolute_position.y -= scroll_offset.children_rect.origin.y;
            }
        }
    }

    let bounds = LogicalRect::new(absolute_position, size);
    generate_background(
        builder,
        bounds,
        positioned_tree.tree.get(node_index).unwrap(),
        dom_id,
    )?;
    generate_border(
        builder,
        bounds,
        positioned_tree.tree.get(node_index).unwrap(),
        dom_id,
    )?;
    Ok(())
}

pub fn generate_combined_text_block(
    builder: &mut DisplayListBuilder,
    base_position: LogicalPosition,
    bounds: &Rect,
    glyphs: &[ShapedGlyph<ParsedFont>],
) -> Result<()> {
    let mut glyph_instances = Vec::new();
    for glyph in glyphs {
        let instance = GlyphInstance {
            point: LogicalPosition::new(
                base_position.x + bounds.x + glyph.offset.x,
                base_position.y + bounds.y + glyph.offset.y,
            ),
            index: glyph.glyph_id as u32,
        };
        glyph_instances.push(instance);
    }
    if !glyph_instances.is_empty() {
        if let Some(first_glyph) = glyphs.first() {
            builder.push_text_run(
                glyph_instances,
                first_glyph.font.clone(),
                ColorU::new(0, 0, 0, 255),
            );
        }
    }
    Ok(())
}

pub fn generate_simple_text(
    builder: &mut DisplayListBuilder,
    bounds: LogicalRect,
    text: &str,
    node: &LayoutNode,
    dom_id: NodeId,
) -> Result<()> {
    let font_info = get_font_info(node, dom_id);
    let color = get_text_color(node, dom_id);
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
            index: ch as u32,
        };
        glyph_instances.push(instance);
    }
    if !glyph_instances.is_empty() {
        builder.push_text_run(glyph_instances, font_info, color);
    }
    Ok(())
}

pub fn generate_background<T: ParsedFontTrait>(
    builder: &mut DisplayListBuilder,
    bounds: LogicalRect,
    node: &LayoutNode<T>,
    _dom_id: NodeId,
) -> Result<()> {
    let background_color = get_background_color(node); // TODO
    if background_color.a > 0 {
        builder.push_rect(bounds, background_color.into());
    }
    // TODO: extract background image, gradient, etc.
    Ok(())
}

pub fn generate_border<T: ParsedFontTrait>(
    builder: &mut DisplayListBuilder,
    bounds: LogicalRect,
    node: &LayoutNode<T>,
    _dom_id: NodeId,
) -> Result<()> {
    let border_info = get_border_info(node);
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
        match &item.item {
            ShapedItem::Cluster(shaped_cluster) => {
                generate_shaped_item(builder, base_position, &item.item)?;
                glyph_count += count_glyphs_in_shaped_item(&item.item);
            }
            ShapedItem::Object {
                content, bounds, ..
            } => {
                generate_inline_object(builder, base_position, &bounds, content)?;
            }
            ShapedItem::Break { source, break_info } => {}
            ShapedItem::Tab { source, bounds } => {}
            ShapedItem::CombinedBlock {
                source,
                glyphs,
                bounds,
                baseline_offset,
            } => {
                // ...
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
            let mut glyph_instances = Vec::new();
            for glyph in &cluster.glyphs {
                let instance = GlyphInstance {
                    point: LogicalPosition::new(
                        base_position.x + cluster.bounds.x + glyph.offset.x,
                        base_position.y + cluster.bounds.y + glyph.offset.y,
                    ),
                    index: glyph.glyph_id as u32,
                };
                glyph_instances.push(instance);
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
            generate_combined_text_block(builder, base_position, bounds, glyphs)?;
        }
        _ => {}
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
    let node = styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| &n.state); // Assuming this exists for style info

    match &node_data.get_node_type() {
        NodeType::Image(image_data) => {
            // Generate image display item
            if let Some(image_key) = get_image_key_for_src(&image_data.get_hash()) {
                builder.push_image(bounds, image_key);
            }
        }
        NodeType::Text(text_data) => {
            // This case should be rare, as text is handled by IFC.
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
            // Other inline content types: Shape, Space, LineBreak
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
                builder.push_rect(bounds, *fill);
            }
        }
        ShapeDefinition::Circle { radius } => {
            // Approximate circle with rounded rectangle
            if let Some(fill) = &shape.fill {
                builder.push_rounded_rect(bounds, *radius, *fill);
            }
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
    positioned_tree: &PositionedTree<T>,
    node_index: usize,
) -> bool {
    let Some(node) = positioned_tree.tree.get(node_index) else {
        return false;
    };
    let Some(dom_id) = node.dom_node_id else {
        return false;
    };
    let position = get_position_type(ctx.styled_dom, Some(dom_id));
    let is_positioned = position == PositionType::Absolute
        || position == PositionType::Relative
        || position == PositionType::Fixed;
    if is_positioned
        && (get_z_index(ctx.styled_dom, Some(dom_id)) != 0
            || position == PositionType::Absolute
            || position == PositionType::Fixed)
    {
        return true;
    }

    // Other conditions that create stacking contexts (not needed for this test):
    // - opacity < 1
    // - transform != none
    // - filter != none
    // - etc.
    // TODO: Implement full stacking context rules.

    false
}

pub fn get_z_index(_styled_dom: &StyledDom, _dom_id: Option<NodeId>) -> i32 {
    0
}
pub fn get_background_color<T: ParsedFontTrait>(_node: &LayoutNode<T>) -> ColorU {
    ColorU::new(255, 255, 255, 0)
}
pub fn get_border_info<T: ParsedFontTrait>(_node: &LayoutNode<T>) -> BorderInfo {
    BorderInfo {
        width: 0.0,
        color: ColorU::new(0, 0, 0, 255),
    }
}
pub fn get_font_info<T: ParsedFontTrait>(_node: &LayoutNode<T>, _dom_id: NodeId) -> FontRef {
    FontRef {
        family: "serif".to_string(),
        weight: FcWeight::Normal,
        style: FontStyle::Normal,
        unicode_ranges: Vec::new(),
    }
}
pub fn get_text_color<T: ParsedFontTrait>(_node: &LayoutNode<T>, _dom_id: NodeId) -> ColorU {
    ColorU::new(0, 0, 0, 255)
}
pub fn get_image_key_for_src(_src: &ImageRefHash) -> Option<ImageKey> {
    None
}
pub fn get_image_key_for_image_source(_source: &ImageSource) -> Option<ImageKey> {
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
