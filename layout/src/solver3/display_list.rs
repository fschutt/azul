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

use allsorts::glyph_position;
use azul_core::{
    dom::{NodeId, NodeType, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    resources::{ImageKey, ImageRefHash},
    selection::{Selection, SelectionState},
    styled_dom::StyledDom,
    ui_solver::GlyphInstance,
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::ColorU,
        layout::{LayoutOverflow, LayoutPosition},
        property::{CssProperty, CssPropertyType},
    },
    LayoutDebugMessage,
};

use crate::{
    solver3::{
        getters::{
            get_background_color, get_border_info, get_border_radius, get_caret_style,
            get_overflow_x, get_overflow_y, get_scrollbar_info_from_layout, get_selection_style,
            get_z_index, BorderInfo, CaretStyle, SelectionStyle,
        },
        layout_tree::{LayoutNode, LayoutTree},
        positioning::get_position_type,
        scrollbar::ScrollbarInfo,
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
    SelectionRect {
        bounds: LogicalRect,
        border_radius: BorderRadius,
        color: ColorU,
    },
    CursorRect {
        bounds: LogicalRect,
        color: ColorU,
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
        font_size_px: f32,
        font_hash: u64,
        color: ColorU,
        clip_rect: LogicalRect,
    },
    Image {
        bounds: LogicalRect,
        key: ImageKey,
    },
    /// A dedicated primitive for a scrollbar with optional GPU-animated opacity.
    ScrollBar {
        bounds: LogicalRect,
        color: ColorU,
        orientation: ScrollbarOrientation,
        /// Optional opacity key for GPU-side fading animation.
        /// If present, the renderer will use this key to look up dynamic opacity.
        /// If None, the alpha channel of `color` is used directly.
        opacity_key: Option<azul_core::resources::OpacityKey>,
        /// Optional hit-test ID for WebRender hit-testing.
        /// If present, allows event handlers to identify which scrollbar component was clicked.
        hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    },

    /// An embedded IFrame that references a child DOM with its own display list.
    /// This mirrors webrender's IframeDisplayItem. The renderer will look up
    /// the child display list by child_dom_id and render it within the bounds.
    IFrame {
        /// The DomId of the child DOM (similar to webrender's pipeline_id)
        child_dom_id: azul_core::dom::DomId,
        /// The bounds where the IFrame should be rendered
        bounds: LogicalRect,
        /// The clip rect for the IFrame content
        clip_rect: LogicalRect,
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

impl BorderRadius {
    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0
            && self.top_right == 0.0
            && self.bottom_left == 0.0
            && self.bottom_right == 0.0
    }
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

    pub fn push_hit_test_area(&mut self, bounds: LogicalRect, tag: TagId) {
        self.items
            .push(DisplayListItem::HitTestArea { bounds, tag });
    }
    pub fn push_scrollbar(
        &mut self,
        bounds: LogicalRect,
        color: ColorU,
        orientation: ScrollbarOrientation,
        opacity_key: Option<azul_core::resources::OpacityKey>,
        hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    ) {
        if color.a > 0 || opacity_key.is_some() {
            // Optimization: Don't draw fully transparent items without opacity keys.
            self.items.push(DisplayListItem::ScrollBar {
                bounds,
                color,
                orientation,
                opacity_key,
                hit_id,
            });
        }
    }
    pub fn push_rect(&mut self, bounds: LogicalRect, color: ColorU, border_radius: BorderRadius) {
        if color.a > 0 {
            // Optimization: Don't draw fully transparent items.
            self.items.push(DisplayListItem::Rect {
                bounds,
                color,
                border_radius,
            });
        }
    }
    pub fn push_selection_rect(
        &mut self,
        bounds: LogicalRect,
        color: ColorU,
        border_radius: BorderRadius,
    ) {
        if color.a > 0 {
            self.items.push(DisplayListItem::SelectionRect {
                bounds,
                color,
                border_radius,
            });
        }
    }

    pub fn push_cursor_rect(&mut self, bounds: LogicalRect, color: ColorU) {
        if color.a > 0 {
            self.items
                .push(DisplayListItem::CursorRect { bounds, color });
        }
    }
    pub fn push_clip(&mut self, bounds: LogicalRect, border_radius: BorderRadius) {
        self.items.push(DisplayListItem::PushClip {
            bounds,
            border_radius,
        });
    }
    pub fn pop_clip(&mut self) {
        self.items.push(DisplayListItem::PopClip);
    }
    pub fn push_scroll_frame(
        &mut self,
        clip_bounds: LogicalRect,
        content_size: LogicalSize,
        scroll_id: ExternalScrollId,
    ) {
        self.items.push(DisplayListItem::PushScrollFrame {
            clip_bounds,
            content_size,
            scroll_id,
        });
    }
    pub fn pop_scroll_frame(&mut self) {
        self.items.push(DisplayListItem::PopScrollFrame);
    }
    pub fn push_border(
        &mut self,
        bounds: LogicalRect,
        color: ColorU,
        width: f32,
        border_radius: BorderRadius,
    ) {
        if color.a > 0 && width > 0.0 {
            self.items.push(DisplayListItem::Border {
                bounds,
                color,
                width,
                border_radius,
            });
        }
    }

    pub fn push_text_run(
        &mut self,
        glyphs: Vec<GlyphInstance>,
        font: FontRef,
        font_size_px: f32,
        font_hash: u64,
        color: ColorU,
        clip_rect: LogicalRect,
    ) {
        if !glyphs.is_empty() && color.a > 0 {
            self.items.push(DisplayListItem::Text {
                glyphs,
                font,
                font_size_px,
                font_hash,
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
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    dom_id: azul_core::dom::DomId,
) -> Result<DisplayList> {
    ctx.debug_log("Generating display list");

    let positioned_tree = PositionedTree {
        tree,
        absolute_positions,
    };
    let mut generator = DisplayListGenerator::new(
        ctx,
        scroll_offsets,
        &positioned_tree,
        gpu_value_cache,
        dom_id,
    );
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
    gpu_value_cache: Option<&'a azul_core::gpu::GpuValueCache>,
    dom_id: azul_core::dom::DomId,
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
        gpu_value_cache: Option<&'a azul_core::gpu::GpuValueCache>,
        dom_id: azul_core::dom::DomId,
    ) -> Self {
        Self {
            ctx,
            scroll_offsets,
            positioned_tree,
            gpu_value_cache,
            dom_id,
        }
    }

    /// Helper to get styled node state for a node
    fn get_styled_node_state(&self, dom_id: NodeId) -> azul_core::styled_dom::StyledNodeState {
        self.ctx
            .styled_dom
            .styled_nodes
            .as_container()
            .get(dom_id)
            .map(|n| n.state.clone())
            .unwrap_or_default()
    }

    /// Emits drawing commands for selection and cursor, if any.
    fn paint_selection_and_cursor(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };
        let Some(layout) = &node.inline_layout_result else {
            return Ok(());
        };

        // Get the selection state for this DOM
        let Some(selection_state) = self.ctx.selections.get(&self.ctx.styled_dom.dom_id) else {
            return Ok(());
        };

        // Check if this selection state applies to the current node
        if selection_state.node_id.node.into_crate_internal() != Some(dom_id) {
            return Ok(());
        }

        // Get the absolute position of this node
        let node_pos = self
            .positioned_tree
            .absolute_positions
            .get(&node_index)
            .copied()
            .unwrap_or_default();

        // Iterate through all selections (multi-cursor/multi-selection support)
        for selection in &selection_state.selections {
            match &selection {
                Selection::Cursor(cursor) => {
                    // Draw cursor
                    if let Some(mut rect) = layout.get_cursor_rect(cursor) {
                        let style = get_caret_style(self.ctx.styled_dom, Some(dom_id));

                        // Adjust rect to absolute position
                        rect.origin.x += node_pos.x;
                        rect.origin.y += node_pos.y;

                        // TODO: The blinking logic would need to be handled by the renderer
                        // using an opacity key or similar, or by the main loop toggling this.
                        // For now, we just draw it.
                        builder.push_cursor_rect(rect, style.color);
                    }
                }
                Selection::Range(range) => {
                    // Draw selection range
                    let rects = layout.get_selection_rects(range);
                    let style = get_selection_style(self.ctx.styled_dom, Some(dom_id));

                    // Convert f32 radius to BorderRadius
                    let border_radius = BorderRadius {
                        top_left: style.radius,
                        top_right: style.radius,
                        bottom_left: style.radius,
                        bottom_right: style.radius,
                    };

                    for mut rect in rects {
                        // Adjust rect to absolute position
                        rect.origin.x += node_pos.x;
                        rect.origin.y += node_pos.y;
                        builder.push_selection_rect(rect, style.bg_color, border_radius);
                    }
                }
            }
        }

        Ok(())
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
        let did_push_clip_or_scroll = self.push_node_clips(builder, context.node_index, node)?;

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
        // 1. Paint the node's background and border.
        self.paint_node_background_and_border(builder, node_index)?;

        // 2. Paint selection highlights and the text cursor if applicable.
        self.paint_selection_and_cursor(builder, node_index)?;

        // 3. Paint the node's own content (text, images, hit-test areas).
        self.paint_node_content(builder, node_index)?;

        // 4. Recursively paint the in-flow children.
        for &child_index in children_indices {
            let child_node = self
                .positioned_tree
                .tree
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;

            // Before painting the child, push its clips.
            let did_push_clip = self.push_node_clips(builder, child_index, child_node)?;

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

    /// Checks if a node requires clipping or scrolling and pushes the appropriate commands.
    /// Returns true if any command was pushed.
    fn push_node_clips(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
        node: &LayoutNode<T>,
    ) -> Result<bool> {
        let Some(dom_id) = node.dom_node_id else {
            return Ok(false);
        };

        let styled_node_state = self.get_styled_node_state(dom_id);

        let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
        let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);
        let border_radius = get_border_radius(self.ctx.styled_dom, dom_id, &styled_node_state);

        let needs_clip = overflow_x.is_clipped() || overflow_y.is_clipped();

        if needs_clip {
            let paint_rect = self.get_paint_rect(node_index).unwrap_or_default();

            let border = &node.box_props.border;
            let clip_rect = LogicalRect {
                origin: LogicalPosition {
                    x: paint_rect.origin.x + border.left,
                    y: paint_rect.origin.y + border.top,
                },
                size: LogicalSize {
                    width: (paint_rect.size.width - border.left - border.right).max(0.0),
                    height: (paint_rect.size.height - border.top - border.bottom).max(0.0),
                },
            };

            if overflow_x.is_scroll() || overflow_y.is_scroll() {
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
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };

        let styled_node_state = self.get_styled_node_state(dom_id);
        let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
        let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);
        let border_radius = get_border_radius(self.ctx.styled_dom, dom_id, &styled_node_state);

        let needs_clip = matches!(overflow_x, LayoutOverflow::Hidden | LayoutOverflow::Clip)
            || matches!(overflow_y, LayoutOverflow::Hidden | LayoutOverflow::Clip)
            || !border_radius.is_zero();

        if needs_clip {
            if matches!(overflow_x, LayoutOverflow::Auto | LayoutOverflow::Scroll)
                || matches!(overflow_y, LayoutOverflow::Auto | LayoutOverflow::Scroll)
            {
                builder.pop_scroll_frame();
            } else {
                builder.pop_clip();
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

        let border_radius = if let Some(dom_id) = node.dom_node_id {
            let styled_node_state = self.get_styled_node_state(dom_id);
            let bg_color = get_background_color(self.ctx.styled_dom, dom_id, &styled_node_state);
            let border_info = get_border_info::<T>(self.ctx.styled_dom, dom_id, &styled_node_state);
            let border_radius = get_border_radius(self.ctx.styled_dom, dom_id, &styled_node_state);

            builder.push_rect(paint_rect, bg_color, border_radius);
            builder.push_border(
                paint_rect,
                border_info.color,
                border_info.width,
                border_radius,
            );
            border_radius
        } else {
            BorderRadius::default()
        };

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

        // Get node_id for GPU cache lookup
        let node_id = node.dom_node_id;

        if scrollbar_info.needs_vertical {
            // Look up opacity key from GPU cache
            let opacity_key = node_id.and_then(|nid| {
                self.gpu_value_cache.and_then(|cache| {
                    cache
                        .scrollbar_v_opacity_keys
                        .get(&(self.dom_id, nid))
                        .copied()
                })
            });

            // Calculate scrollbar bounds based on paint_rect
            let sb_bounds = LogicalRect {
                origin: LogicalPosition::new(
                    paint_rect.origin.x + paint_rect.size.width - scrollbar_info.scrollbar_width,
                    paint_rect.origin.y,
                ),
                size: LogicalSize::new(scrollbar_info.scrollbar_width, paint_rect.size.height),
            };

            // Generate hit-test ID for vertical scrollbar thumb
            let hit_id = node_id
                .map(|nid| azul_core::hit_test::ScrollbarHitId::VerticalThumb(self.dom_id, nid));

            builder.push_scrollbar(
                sb_bounds,
                ColorU::new(192, 192, 192, 255),
                ScrollbarOrientation::Vertical,
                opacity_key,
                hit_id,
            );
        }
        if scrollbar_info.needs_horizontal {
            // Look up opacity key from GPU cache
            let opacity_key = node_id.and_then(|nid| {
                self.gpu_value_cache.and_then(|cache| {
                    cache
                        .scrollbar_h_opacity_keys
                        .get(&(self.dom_id, nid))
                        .copied()
                })
            });

            let sb_bounds = LogicalRect {
                origin: LogicalPosition::new(
                    paint_rect.origin.x,
                    paint_rect.origin.y + paint_rect.size.height - scrollbar_info.scrollbar_height,
                ),
                size: LogicalSize::new(paint_rect.size.width, scrollbar_info.scrollbar_height),
            };

            // Generate hit-test ID for horizontal scrollbar thumb
            let hit_id = node_id
                .map(|nid| azul_core::hit_test::ScrollbarHitId::HorizontalThumb(self.dom_id, nid));

            builder.push_scrollbar(
                sb_bounds,
                ColorU::new(192, 192, 192, 255),
                ScrollbarOrientation::Horizontal,
                opacity_key,
                hit_id,
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
        // TODO: This will always paint images over the glyphs
        // TODO: Handle z-index within inline content (e.g. background images)
        // TODO: Handle text decorations (underline, strikethrough, etc.)
        // TODO: Handle text shadows
        // TODO: Handle text overflowing (based on container_rect and overflow behavior)
        let glyph_runs = crate::text3::glyphs::get_glyph_runs(layout);

        for glyph_run in glyph_runs {
            let clip_rect = container_rect; // Clip to the container rect
                                            // Use the font_hash from the glyph run for renderer lookups
            let font_ref = FontRef::invalid(); // TODO: Remove this once we fully migrate
            builder.push_text_run(
                glyph_run.glyphs,
                font_ref,
                glyph_run.font_size_px,
                glyph_run.font_hash,
                glyph_run.color,
                clip_rect,
            );
        }

        for item in &layout.items {
            let base_pos = container_rect.origin;
            match &item.item {
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
        if position == LayoutPosition::Absolute || position == LayoutPosition::Fixed {
            return true;
        }

        let z_index = get_z_index(self.ctx.styled_dom, Some(dom_id));
        if position == LayoutPosition::Relative && z_index != 0 {
            return true;
        }

        if let Some(styled_node) = self.ctx.styled_dom.styled_nodes.as_container().get(dom_id) {
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].state;

            // Opacity < 1
            if let Some(opacity_val) = self
                .ctx
                .styled_dom
                .css_property_cache
                .ptr
                .get_opacity(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property())
            {
                if opacity_val.inner.normalized() < 1.0 {
                    return true;
                }
            }

            // Transform != none
            if let Some(transform_val) = self
                .ctx
                .styled_dom
                .css_property_cache
                .ptr
                .get_transform(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property())
            {
                if !transform_val.is_empty() {
                    return true;
                }
            }
        }

        false
    }
}

/// Helper struct to pass layout results to the generator.
pub struct PositionedTree<'a, T: ParsedFontTrait> {
    pub tree: &'a LayoutTree<T>,
    pub absolute_positions: &'a BTreeMap<usize, LogicalPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowBehavior {
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}

impl OverflowBehavior {
    pub fn is_clipped(&self) -> bool {
        matches!(self, Self::Hidden | Self::Clip | Self::Scroll | Self::Auto)
    }

    pub fn is_scroll(&self) -> bool {
        matches!(self, Self::Scroll | Self::Auto)
    }
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

fn get_image_key_for_src(_src: &ImageRefHash) -> Option<ImageKey> {
    None
}
fn get_image_key_for_image_source(_source: &ImageSource) -> Option<ImageKey> {
    None
}
