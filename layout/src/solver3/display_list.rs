//! solver3/display_list.rs
//!
//! Pass 4: Generate a renderer-agnostic display list from a laid-out tree.
//! The translation layer to WebRender would look something like this (in pseudocode):
//!
//! ```rust,no_run,ignore
//! // In the WebRender translation layer
//! fn translate_to_webrender(display_list: &DisplayList, builder: &mut WrDisplayListBuilder) {
//!     for item in &display_list.items {
//!         match item {
//!             DisplayListItem::Rect { bounds, color, border_radius } => {
//!                 // ... push_rect with current spatial_id and clip_id
//!             }
//!             DisplayListItem::PushClip { bounds, border_radius } => {
//!                 // let new_clip_id = builder.define_clip_rounded_rect(...);
//!                 // clip_stack.push(new_clip_id);
//!             }
//!             DisplayListItem::PopClip => {
//!                 // clip_stack.pop();
//!             }
//!             DisplayListItem::PushScrollFrame { clip_bounds, content_size, scroll_id } => {
//!                 // let new_space_and_clip = builder.define_scroll_frame(...);
//!                 // spatial_stack.push(new_space_and_clip.spatial_id);
//!                 // clip_stack.push(new_space_and_clip.clip_id);
//!             }
//!             DisplayListItem::PopScrollFrame => {
//!                 // spatial_stack.pop();
//!                 // clip_stack.pop();
//!             }
//!             DisplayListItem::HitTestArea { bounds, tag } => {
//!                 // builder.push_hit_test(...);
//!             }
//!             // ... and so on for other primitives
//!         }
//!     }
//! }
//! ```
use std::collections::BTreeMap;

use azul_core::{
    app_resources::{ImageKey, ImageRefHash},
    callbacks::ScrollPosition,
    display_list::GlyphInstance, // Use the core GlyphInstance definition
    dom::{NodeId, NodeType},
    styled_dom::StyledDom,
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::{ColorU, LayoutDebugMessage};

use crate::{
    solver3::{
        layout_tree::{LayoutNode, LayoutTree},
        positioning::{get_position_type, PositionType},
        LayoutContext, LayoutError, Result,
    },
    text3::cache::{
        FontLoaderTrait, FontRef, ImageSource, InlineContent, ParsedFontTrait, ShapedItem,
        UnifiedLayout,
    },
};

/// The final, renderer-agnostic output of the layout engine.
///
/// This is a flat list of drawing and state-management commands, already sorted
/// according to the CSS paint order. A renderer can consume this list directly.
#[derive(Debug, Default)]
pub struct DisplayList {
    pub items: Vec<DisplayListItem>,
}

/// A command in the display list. Can be either a drawing primitive or a
/// state-management instruction for the renderer's graphics context.
#[derive(Debug)]
pub enum DisplayListItem {
    // --- Drawing Primitives ---
    Rect {
        bounds: LogicalRect,
        color: ColorU,
        border_radius: BorderRadius,
    },
    Border {
        bounds: LogicalRect,
        color: ColorU,
        width: f32,
        border_radius: BorderRadius,
    },
    Text {
        glyphs: Vec<GlyphInstance>,
        font: FontRef,
        color: ColorU,
        clip_rect: LogicalRect,
    },
    Image {
        bounds: LogicalRect,
        key: ImageKey,
    },
    /// A dedicated primitive for a scrollbar.
    ScrollBar {
        bounds: LogicalRect,
        color: ColorU,
        orientation: ScrollbarOrientation,
    },

    // --- State-Management Commands ---
    /// Pushes a new clipping rectangle onto the renderer's clip stack.
    /// All subsequent primitives will be clipped by this rect until a PopClip.
    PushClip {
        bounds: LogicalRect,
        border_radius: BorderRadius,
    },
    /// Pops the current clip from the renderer's clip stack.
    PopClip,

    /// Defines a scrollable area. This is a specialized clip that also
    /// establishes a new coordinate system for its children, which can be offset.
    PushScrollFrame {
        /// The clip rect in the parent's coordinate space.
        clip_bounds: LogicalRect,
        /// The total size of the scrollable content.
        content_size: LogicalSize,
        /// An ID for the renderer to track this scrollable area between frames.
        scroll_id: ExternalScrollId, // This would be a renderer-agnostic ID type
    },
    /// Pops the current scroll frame.
    PopScrollFrame,

    /// Defines a region for hit-testing.
    HitTestArea {
        bounds: LogicalRect,
        tag: TagId, // This would be a renderer-agnostic ID type
    },
}

// Helper structs for the DisplayList
#[derive(Debug, Copy, Clone, Default)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_left: f32,
    pub bottom_right: f32,
}

#[derive(Debug, Copy, Clone)]
pub enum ScrollbarOrientation {
    Horizontal,
    Vertical,
}

// Dummy types for compilation
pub type ExternalScrollId = u64;
pub type TagId = u64;

/// Internal builder to accumulate display list items during generation.
#[derive(Debug, Default)]
struct DisplayListBuilder {
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
        if color.a > 0 {
            // Optimization: Don't draw fully transparent items.
            self.items.push(DisplayListItem::Rect { bounds, color });
        }
    }

    pub fn push_border(&mut self, bounds: LogicalRect, color: ColorU, width: f32) {
        if color.a > 0 && width > 0.0 {
            self.items.push(DisplayListItem::Border {
                bounds,
                color,
                width,
            });
        }
    }

    pub fn push_text_run(
        &mut self,
        glyphs: Vec<GlyphInstance>,
        font: FontRef,
        color: ColorU,
        clip_rect: LogicalRect,
    ) {
        if !glyphs.is_empty() && color.a > 0 {
            self.items.push(DisplayListItem::Text {
                glyphs,
                font,
                color,
                clip_rect,
            });
        }
    }

    pub fn push_image(&mut self, bounds: LogicalRect, key: ImageKey) {
        self.items.push(DisplayListItem::Image { bounds, key });
    }
}

/// Main entry point for generating the display list.
pub fn generate_display_list<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &LayoutTree<T>,
    absolute_positions: &BTreeMap<usize, LogicalPosition>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
) -> Result<DisplayList> {
    ctx.debug_log("Generating display list");

    let positioned_tree = PositionedTree {
        tree,
        absolute_positions,
    };
    let mut generator = DisplayListGenerator::new(ctx, scroll_offsets, &positioned_tree);
    let mut builder = DisplayListBuilder::new();

    // 1. Build a tree of stacking contexts, which defines the global paint order.
    let stacking_context_tree = generator.collect_stacking_contexts(tree.root)?;

    // 2. Traverse the stacking context tree to generate display items in the correct order.
    generator.generate_for_stacking_context(&mut builder, &stacking_context_tree)?;

    let display_list = builder.build();
    ctx.debug_log(&format!(
        "Generated display list with {} items",
        display_list.items.len()
    ));
    Ok(display_list)
}

/// A helper struct that holds all necessary state and context for the generation process.
struct DisplayListGenerator<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> {
    ctx: &'a LayoutContext<'b, T, Q>,
    scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>,
    positioned_tree: &'a PositionedTree<'a, T>,
}

/// Represents a node in the CSS stacking context tree, not the DOM tree.
#[derive(Debug)]
struct StackingContext {
    node_index: usize,
    z_index: i32,
    child_contexts: Vec<StackingContext>,
    /// Children that do not create their own stacking contexts and are painted in DOM order.
    in_flow_children: Vec<usize>,
}

impl<'a, 'b, T, Q> DisplayListGenerator<'a, 'b, T, Q>
where
    T: ParsedFontTrait,
    Q: FontLoaderTrait<T>,
{
    pub fn new(
        ctx: &'a LayoutContext<'b, T, Q>,
        scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>,
        positioned_tree: &'a PositionedTree<'a, T>,
    ) -> Self {
        Self {
            ctx,
            scroll_offsets,
            positioned_tree,
        }
    }

    /// Recursively builds the tree of stacking contexts starting from a given layout node.
    fn collect_stacking_contexts(&self, node_index: usize) -> Result<StackingContext> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;
        let z_index = get_z_index(self.ctx.styled_dom, node.dom_node_id);

        let mut child_contexts = Vec::new();
        let mut in_flow_children = Vec::new();

        for &child_index in &node.children {
            if self.establishes_stacking_context(child_index) {
                child_contexts.push(self.collect_stacking_contexts(child_index)?);
            } else {
                in_flow_children.push(child_index);
            }
        }

        Ok(StackingContext {
            node_index,
            z_index,
            child_contexts,
            in_flow_children,
        })
    }

    /// Recursively traverses the stacking context tree, emitting drawing commands to the builder
    /// according to the CSS Painting Algorithm specification.
    fn generate_for_stacking_context(
        &self,
        builder: &mut DisplayListBuilder,
        context: &StackingContext,
    ) -> Result<()> {
        // Before painting the node, check if it establishes a new clip or scroll frame.
        let node = self
            .positioned_tree
            .tree
            .get(context.node_index)
            .ok_or(LayoutError::InvalidTree)?;
        let did_push_clip_or_scroll = self.push_node_clips(builder, node)?;

        // 1. Paint background and borders for the context's root element.
        self.paint_node_background_and_border(builder, context.node_index)?;

        // 2. Paint child stacking contexts with negative z-indices.
        let mut negative_z_children: Vec<_> = context
            .child_contexts
            .iter()
            .filter(|c| c.z_index < 0)
            .collect();
        negative_z_children.sort_by_key(|c| c.z_index);
        for child in negative_z_children {
            self.generate_for_stacking_context(builder, child)?;
        }

        // 3. Paint the in-flow descendants of the context root.
        self.paint_in_flow_descendants(builder, context.node_index, &context.in_flow_children)?;

        // 4. Paint child stacking contexts with z-index: 0 / auto.
        for child in context.child_contexts.iter().filter(|c| c.z_index == 0) {
            self.generate_for_stacking_context(builder, child)?;
        }

        // 5. Paint child stacking contexts with positive z-indices.
        let mut positive_z_children: Vec<_> = context
            .child_contexts
            .iter()
            .filter(|c| c.z_index > 0)
            .collect();

        positive_z_children.sort_by_key(|c| c.z_index);

        for child in positive_z_children {
            self.generate_for_stacking_context(builder, child)?;
        }

        // After painting the node and all its descendants, pop any contexts it pushed.
        if did_push_clip_or_scroll {
            self.pop_node_clips(builder, node)?;
        }

        Ok(())
    }

    /// Paints the content and non-stacking-context children.
    fn paint_in_flow_descendants(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
        children_indices: &[usize],
    ) -> Result<()> {
        // 1. Paint the node's own content (text, images, hit-test areas).
        self.paint_node_content(builder, node_index)?;

        // 2. Recursively paint the in-flow children.
        for &child_index in children_indices {
            let child_node = self
                .positioned_tree
                .tree
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;

            // Before painting the child, push its clips.
            let did_push_clip = self.push_node_clips(builder, child_node)?;

            // Paint the child's background, border, content, and then its own children.
            self.paint_node_background_and_border(builder, child_index)?;
            self.paint_in_flow_descendants(builder, child_index, &child_node.children)?;

            // Pop the child's clips.
            if did_push_clip {
                self.pop_node_clips(builder, child_node)?;
            }
        }
        Ok(())
    }

    /// Calculates the final paint-time rectangle for a node, accounting for parent scroll offsets.
    fn get_paint_rect(&self, node_index: usize) -> Option<LogicalRect> {
        let node = self.positioned_tree.tree.get(node_index)?;
        let mut pos = self
            .positioned_tree
            .absolute_positions
            .get(&node_index)
            .copied()
            .unwrap_or_default();
        let size = node.used_size.unwrap_or_default();

        if let Some(parent_idx) = node.parent {
            if let Some(parent_dom_id) = self
                .positioned_tree
                .tree
                .get(parent_idx)
                .and_then(|p| p.dom_node_id)
            {
                if let Some(scroll) = self.scroll_offsets.get(&parent_dom_id) {
                    pos.x -= scroll.children_rect.origin.x;
                    pos.y -= scroll.children_rect.origin.y;
                }
            }
        }
        Some(LogicalRect::new(pos, size))
    }

    /// Checks if a node requires clipping or scrolling and pushes the appropriate commands.
    /// Returns true if any command was pushed.
    fn push_node_clips(
        &self,
        builder: &mut DisplayListBuilder,
        node: &LayoutNode<T>,
    ) -> Result<bool> {
        let overflow = get_overflow_behavior(self.ctx.styled_dom, node.dom_node_id);
        let border_radius = get_border_radius(self.ctx.styled_dom, node.dom_node_id);

        let needs_clip = overflow.is_clipped() || !border_radius.is_zero();

        if needs_clip {
            let clip_rect = self.get_paint_rect(node.parent.unwrap_or(0)) // This needs careful implementation to get the right rect
                .unwrap_or_default(); // Should be the padding box of the node.

            if overflow.is_scroll() {
                // It's a scroll frame
                let scroll_id = get_scroll_id(node.dom_node_id); // Unique ID for this scrollable area
                let content_size = get_scroll_content_size(node); // From layout phase
                builder.push_scroll_frame(clip_rect, content_size, scroll_id);
            } else {
                // It's a simple clip
                builder.push_clip(clip_rect, border_radius);
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Pops any clip/scroll commands associated with a node.
    fn pop_node_clips(&self, builder: &mut DisplayListBuilder, node: &LayoutNode<T>) -> Result<()> {
        let overflow = get_overflow_behavior(self.ctx.styled_dom, node.dom_node_id);
        let border_radius = get_border_radius(self.ctx.styled_dom, node.dom_node_id);
        let needs_clip = overflow.is_clipped() || !border_radius.is_zero();

        if needs_clip {
            if overflow.is_scroll() {
                builder.pop_scroll_frame();
            } else {
                builder.pop_clip();
            }
        }
        Ok(())
    }

    /// Emits drawing commands for the background and border of a single node.
    fn paint_node_background_and_border(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;

        // STUB: These should read from the styled DOM's computed values.
        let bg_color = get_background_color(node);
        let border_info = get_border_info(node);

        builder.push_rect(paint_rect, bg_color);
        builder.push_border(paint_rect, border_info.color, border_info.width);
        Ok(())
    }

    /// Emits drawing commands for the foreground content, including hit-test areas and scrollbars.
    fn paint_node_content(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;
        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };

        // Add a hit-test area for this node if it's interactive.
        if let Some(tag_id) = get_tag_id(self.ctx.styled_dom, node.dom_node_id) {
            builder.push_hit_test_area(paint_rect, tag_id);
        }

        // Paint the node's visible content.
        if let Some(inline_layout) = &node.inline_layout_result {
            self.paint_inline_content(builder, paint_rect, inline_layout)?;
        } else if let Some(dom_id) = node.dom_node_id {
            // This node might be a simple replaced element, like an <img> tag.
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            if let NodeType::Image(image_data) = node_data.get_node_type() {
                if let Some(image_key) = get_image_key_for_src(&image_data.get_hash()) {
                    builder.push_image(paint_rect, image_key);
                }
            }
        }

        // Check if we need to draw scrollbars for this node.
        let scrollbar_info = get_scrollbar_info_from_layout(node); // This data would be cached from the layout phase.
        if scrollbar_info.needs_vertical {
            // Calculate scrollbar bounds based on paint_rect
            let sb_bounds = LogicalRect {
                origin: LogicalPosition::new(
                    paint_rect.origin.x + paint_rect.size.width - scrollbar_info.scrollbar_width,
                    paint_rect.origin.y,
                ),
                size: LogicalSize::new(scrollbar_info.scrollbar_width, paint_rect.size.height),
            };
            builder.push_scrollbar(
                sb_bounds,
                ColorU::new(192, 192, 192, 255),
                ScrollbarOrientation::Vertical,
            );
        }
        if scrollbar_info.needs_horizontal {
            let sb_bounds = LogicalRect {
                origin: LogicalPosition::new(
                    paint_rect.origin.x,
                    paint_rect.origin.y + paint_rect.size.height - scrollbar_info.scrollbar_height,
                ),
                size: LogicalSize::new(paint_rect.size.width, scrollbar_info.scrollbar_height),
            };
            builder.push_scrollbar(
                sb_bounds,
                ColorU::new(192, 192, 192, 255),
                ScrollbarOrientation::Horizontal,
            );
        }

        Ok(())
    }

    /// Converts the rich layout information from `text3` into drawing commands.
    fn paint_inline_content(
        &self,
        builder: &mut DisplayListBuilder,
        container_rect: LogicalRect,
        layout: &UnifiedLayout<T>,
    ) -> Result<()> {
        for item in &layout.items {
            let base_pos = container_rect.origin;
            match &item.item {
                ShapedItem::Cluster(cluster) => {
                    let mut glyph_instances = Vec::new();
                    for glyph in &cluster.glyphs {
                        let instance = GlyphInstance {
                            point: LogicalPosition::new(
                                base_pos.x + cluster.bounds.x + glyph.offset.x,
                                base_pos.y + cluster.bounds.y + glyph.offset.y,
                            ),
                            index: glyph.glyph_id as u32,
                            size: LogicalSize::new(0.0, 0.0), // Size often implicit in font metrics
                        };
                        glyph_instances.push(instance);
                    }
                    if !glyph_instances.is_empty() {
                        builder.push_text_run(
                            glyph_instances,
                            cluster.style.font_ref.clone(),
                            cluster.style.color.into(),
                            container_rect, // Text is clipped by its containing block.
                        );
                    }
                }
                ShapedItem::Object {
                    content, bounds, ..
                } => {
                    let object_bounds = LogicalRect::new(
                        LogicalPosition::new(base_pos.x + bounds.x, base_pos.y + bounds.y),
                        LogicalSize::new(bounds.width, bounds.height),
                    );
                    if let InlineContent::Image(image) = content {
                        if let Some(image_key) = get_image_key_for_image_source(&image.source) {
                            builder.push_image(object_bounds, image_key);
                        }
                    }
                }
                _ => {} // Other item types (e.g., breaks) don't produce painted output.
            }
        }
        Ok(())
    }

    /// Determines if a node establishes a new stacking context based on CSS rules.
    fn establishes_stacking_context(&self, node_index: usize) -> bool {
        let Some(node) = self.positioned_tree.tree.get(node_index) else {
            return false;
        };
        let Some(dom_id) = node.dom_node_id else {
            return false;
        };

        let position = get_position_type(self.ctx.styled_dom, Some(dom_id));
        if position == PositionType::Absolute || position == PositionType::Fixed {
            return true;
        }
        if position == PositionType::Relative && get_z_index(self.ctx.styled_dom, Some(dom_id)) != 0
        {
            return true;
        }
        // FULL IMPLEMENTATION: Add checks for opacity < 1, transform != none, filter != none, etc.
        false
    }
}

/// Helper struct to pass layout results to the generator.
pub struct PositionedTree<'a, T: ParsedFontTrait> {
    pub tree: &'a LayoutTree<T>,
    pub absolute_positions: &'a BTreeMap<usize, LogicalPosition>,
}

// STUB functions for reading style properties. These should be replaced
// with calls to a real computed style cache.
struct OverflowBehavior {
    x: bool,
    y: bool,
}
impl OverflowBehavior {
    fn is_clipped(&self) -> bool {
        self.x || self.y
    }
    fn is_scroll(&self) -> bool {
        self.x || self.y
    } // Simplified
}
fn get_overflow_behavior(dom: &StyledDom, id: Option<NodeId>) -> OverflowBehavior {
    OverflowBehavior { x: false, y: false }
}
fn get_border_radius(dom: &StyledDom, id: Option<NodeId>) -> BorderRadius {
    BorderRadius::default()
}
fn get_scroll_id(id: Option<NodeId>) -> ExternalScrollId {
    id.map(|i| i.index() as u64).unwrap_or(0)
}
fn get_scroll_content_size<T: ParsedFontTrait>(node: &LayoutNode<T>) -> LogicalSize {
    node.used_size.unwrap_or_default()
}
fn get_tag_id(dom: &StyledDom, id: Option<NodeId>) -> Option<TagId> {
    id.map(|i| i.index() as u64)
}
struct ScrollbarInfo {
    needs_vertical: bool,
    needs_horizontal: bool,
    scrollbar_width: f32,
    scrollbar_height: f32,
}
fn get_scrollbar_info_from_layout<T: ParsedFontTrait>(node: &LayoutNode<T>) -> ScrollbarInfo {
    ScrollbarInfo {
        needs_vertical: false,
        needs_horizontal: false,
        scrollbar_width: 16.0,
        scrollbar_height: 16.0,
    }
}

fn get_z_index(_styled_dom: &StyledDom, _dom_id: Option<NodeId>) -> i32 {
    0
}
fn get_background_color<T: ParsedFontTrait>(_node: &LayoutNode<T>) -> ColorU {
    ColorU::new(255, 255, 255, 0)
} // Default transparent
fn get_border_info<T: ParsedFontTrait>(_node: &LayoutNode<T>) -> BorderInfo {
    BorderInfo {
        width: 0.0,
        color: ColorU::new(0, 0, 0, 255),
    }
}
fn get_image_key_for_src(_src: &ImageRefHash) -> Option<ImageKey> {
    None
}
fn get_image_key_for_image_source(_source: &ImageSource) -> Option<ImageKey> {
    None
}

#[derive(Debug)]
pub struct BorderInfo {
    pub width: f32,
    pub color: ColorU,
}
