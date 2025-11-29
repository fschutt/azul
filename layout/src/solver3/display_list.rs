//! solver3/display_list.rs
//!
//! Pass 4: Generate a renderer-agnostic display list from a laid-out tree.
//!
//! NOTE: This file uses deprecated ctx.debug_*() methods.
//! TODO: Migrate to debug_*!() macros for lazy evaluation.
//!
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

#![allow(deprecated)]

use std::collections::BTreeMap;

use allsorts::glyph_position;
use azul_core::{
    dom::{NodeId, NodeType, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    resources::{IdNamespace, ImageKey, ImageRefHash},
    selection::{Selection, SelectionState},
    styled_dom::StyledDom,
    ui_solver::GlyphInstance,
};
use azul_css::{
    css::CssPropertyValue,
    format_rust_code::GetHash,
    props::{
        basic::{ColorU, FontRef},
        layout::{LayoutOverflow, LayoutPosition},
        property::{CssProperty, CssPropertyType},
        style::{
            border_radius::StyleBorderRadius, LayoutBorderBottomWidth, LayoutBorderLeftWidth,
            LayoutBorderRightWidth, LayoutBorderTopWidth, StyleBorderBottomColor,
            StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
            StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderTopStyle,
        },
    },
    LayoutDebugMessage,
};

use crate::{debug_info};
use crate::{
    font_traits::{
        FontHash, FontLoaderTrait, ImageSource, InlineContent, ParsedFontTrait, ShapedItem,
        UnifiedLayout,
    },
    solver3::{
        getters::{
            get_background_color, get_border_info, get_border_radius, get_caret_style,
            get_overflow_x, get_overflow_y, get_scrollbar_info_from_layout, get_selection_style,
            get_style_border_radius, get_z_index, BorderInfo, CaretStyle, SelectionStyle,
        },
        layout_tree::{LayoutNode, LayoutTree},
        positioning::get_position_type,
        scrollbar::ScrollbarInfo,
        LayoutContext, LayoutError, Result,
    },
};

#[cfg(feature = "text_layout")]
use crate::text3;
#[cfg(feature = "text_layout")]
use crate::text3::cache::{PositionedItem, InlineShape};
use std::sync::Arc;

/// Border widths for all four sides
#[derive(Debug, Clone, Copy)]
pub struct StyleBorderWidths {
    pub top: Option<CssPropertyValue<LayoutBorderTopWidth>>,
    pub right: Option<CssPropertyValue<LayoutBorderRightWidth>>,
    pub bottom: Option<CssPropertyValue<LayoutBorderBottomWidth>>,
    pub left: Option<CssPropertyValue<LayoutBorderLeftWidth>>,
}

/// Border colors for all four sides
#[derive(Debug, Clone, Copy)]
pub struct StyleBorderColors {
    pub top: Option<CssPropertyValue<StyleBorderTopColor>>,
    pub right: Option<CssPropertyValue<StyleBorderRightColor>>,
    pub bottom: Option<CssPropertyValue<StyleBorderBottomColor>>,
    pub left: Option<CssPropertyValue<StyleBorderLeftColor>>,
}

/// Border styles for all four sides
#[derive(Debug, Clone, Copy)]
pub struct StyleBorderStyles {
    pub top: Option<CssPropertyValue<StyleBorderTopStyle>>,
    pub right: Option<CssPropertyValue<StyleBorderRightStyle>>,
    pub bottom: Option<CssPropertyValue<StyleBorderBottomStyle>>,
    pub left: Option<CssPropertyValue<StyleBorderLeftStyle>>,
}

/// A rectangle in border-box coordinates (includes padding and border).
/// This is what layout calculates and stores in `used_size` and absolute positions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderBoxRect(pub LogicalRect);

/// Simple struct for passing element dimensions to border-radius calculation
#[derive(Debug, Clone, Copy)]
pub struct PhysicalSizeImport {
    pub width: f32,
    pub height: f32,
}

impl BorderBoxRect {
    /// Convert border-box to content-box by subtracting padding and border.
    /// Content-box is where inline layout and text actually render.
    pub fn to_content_box(self, padding: &crate::solver3::geometry::EdgeSizes, border: &crate::solver3::geometry::EdgeSizes) -> ContentBoxRect {
        ContentBoxRect(LogicalRect {
            origin: LogicalPosition {
                x: self.0.origin.x + padding.left + border.left,
                y: self.0.origin.y + padding.top + border.top,
            },
            size: LogicalSize {
                width: self.0.size.width - padding.left - padding.right - border.left - border.right,
                height: self.0.size.height - padding.top - padding.bottom - border.top - border.bottom,
            },
        })
    }

    /// Get the inner LogicalRect
    pub fn rect(&self) -> LogicalRect {
        self.0
    }
}

/// A rectangle in content-box coordinates (excludes padding and border).
/// This is where text and inline content is positioned by the inline formatter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContentBoxRect(pub LogicalRect);

impl ContentBoxRect {
    /// Get the inner LogicalRect
    pub fn rect(&self) -> LogicalRect {
        self.0
    }
}

/// The final, renderer-agnostic output of the layout engine.
///
/// This is a flat list of drawing and state-management commands, already sorted
/// according to the CSS paint order. A renderer can consume this list directly.
#[derive(Debug, Default)]
pub struct DisplayList {
    pub items: Vec<DisplayListItem>,
    /// Optional mapping from item index to the DOM NodeId that generated it.
    /// Used for pagination to look up CSS break properties.
    /// Not all items have a source node (e.g., synthesized decorations).
    pub node_mapping: Vec<Option<azul_core::dom::NodeId>>,
}

/// A command in the display list. Can be either a drawing primitive or a
/// state-management instruction for the renderer's graphics context.
#[derive(Debug, Clone)]
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
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
        border_radius: StyleBorderRadius,
    },
    /// Text layout with full metadata (for PDF, accessibility, etc.)
    /// This is pushed BEFORE the individual Text items and contains
    /// the original text, glyph-to-unicode mapping, and positioning info
    TextLayout {
        layout: Arc<dyn std::any::Any + Send + Sync>, // Type-erased UnifiedLayout
        bounds: LogicalRect,
        font_hash: FontHash,
        font_size_px: f32,
        color: ColorU,
    },
    /// Text rendered with individual glyph positioning (for simple renderers)
    Text {
        glyphs: Vec<GlyphInstance>,
        font_hash: FontHash, // Changed from FontRef - just store the hash
        font_size_px: f32,
        color: ColorU,
        clip_rect: LogicalRect,
    },
    /// Underline decoration for text (CSS text-decoration: underline)
    Underline {
        bounds: LogicalRect,
        color: ColorU,
        thickness: f32,
    },
    /// Strikethrough decoration for text (CSS text-decoration: line-through)
    Strikethrough {
        bounds: LogicalRect,
        color: ColorU,
        thickness: f32,
    },
    /// Overline decoration for text (CSS text-decoration: overline)
    Overline {
        bounds: LogicalRect,
        color: ColorU,
        thickness: f32,
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

    /// Pushes a new stacking context for proper z-index layering.
    /// All subsequent primitives until PopStackingContext will be in this stacking context.
    PushStackingContext {
        /// The z-index for this stacking context (for debugging/validation)
        z_index: i32,
        /// The bounds of the stacking context root element
        bounds: LogicalRect,
    },
    /// Pops the current stacking context.
    PopStackingContext,

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
    node_mapping: Vec<Option<azul_core::dom::NodeId>>,
    /// Current node being processed (set by generator)
    current_node: Option<azul_core::dom::NodeId>,
}

impl DisplayListBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set the current node context for subsequent push operations
    pub fn set_current_node(&mut self, node_id: Option<azul_core::dom::NodeId>) {
        self.current_node = node_id;
    }
    
    /// Push an item and record its node mapping
    fn push_item(&mut self, item: DisplayListItem) {
        self.items.push(item);
        self.node_mapping.push(self.current_node);
    }
    
    pub fn build(self) -> DisplayList {
        DisplayList { 
            items: self.items,
            node_mapping: self.node_mapping,
        }
    }

    pub fn push_hit_test_area(&mut self, bounds: LogicalRect, tag: TagId) {
        self.push_item(DisplayListItem::HitTestArea { bounds, tag });
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
            self.push_item(DisplayListItem::ScrollBar {
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
            self.push_item(DisplayListItem::Rect {
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
            self.push_item(DisplayListItem::SelectionRect {
                bounds,
                color,
                border_radius,
            });
        }
    }

    pub fn push_cursor_rect(&mut self, bounds: LogicalRect, color: ColorU) {
        if color.a > 0 {
            self.push_item(DisplayListItem::CursorRect { bounds, color });
        }
    }
    pub fn push_clip(&mut self, bounds: LogicalRect, border_radius: BorderRadius) {
        self.push_item(DisplayListItem::PushClip {
            bounds,
            border_radius,
        });
    }
    pub fn pop_clip(&mut self) {
        self.push_item(DisplayListItem::PopClip);
    }
    pub fn push_scroll_frame(
        &mut self,
        clip_bounds: LogicalRect,
        content_size: LogicalSize,
        scroll_id: ExternalScrollId,
    ) {
        self.push_item(DisplayListItem::PushScrollFrame {
            clip_bounds,
            content_size,
            scroll_id,
        });
    }
    pub fn pop_scroll_frame(&mut self) {
        self.push_item(DisplayListItem::PopScrollFrame);
    }
    pub fn push_border(
        &mut self,
        bounds: LogicalRect,
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
        border_radius: StyleBorderRadius,
    ) {
        // Check if any border side is visible
        let has_visible_border = {
            let has_width = widths.top.is_some()
                || widths.right.is_some()
                || widths.bottom.is_some()
                || widths.left.is_some();
            let has_style = styles.top.is_some()
                || styles.right.is_some()
                || styles.bottom.is_some()
                || styles.left.is_some();
            has_width && has_style
        };

        if has_visible_border {
            self.push_item(DisplayListItem::Border {
                bounds,
                widths,
                colors,
                styles,
                border_radius,
            });
        }
    }

    pub fn push_stacking_context(&mut self, z_index: i32, bounds: LogicalRect) {
        self.push_item(DisplayListItem::PushStackingContext { z_index, bounds });
    }

    pub fn pop_stacking_context(&mut self) {
        self.push_item(DisplayListItem::PopStackingContext);
    }

    pub fn push_text_run(
        &mut self,
        glyphs: Vec<GlyphInstance>,
        font_hash: FontHash, // Just the hash, not the full FontRef
        font_size_px: f32,
        color: ColorU,
        clip_rect: LogicalRect,
    ) {
        if !glyphs.is_empty() && color.a > 0 {
            self.push_item(DisplayListItem::Text {
                glyphs,
                font_hash,
                font_size_px,
                color,
                clip_rect,
            });
        }
    }

    pub fn push_text_layout(
        &mut self,
        layout: Arc<dyn std::any::Any + Send + Sync>,
        bounds: LogicalRect,
        font_hash: FontHash,
        font_size_px: f32,
        color: ColorU,
    ) {
        if color.a > 0 {
            self.push_item(DisplayListItem::TextLayout {
                layout,
                bounds,
                font_hash,
                font_size_px,
                color,
            });
        }
    }

    pub fn push_underline(&mut self, bounds: LogicalRect, color: ColorU, thickness: f32) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Underline {
                bounds,
                color,
                thickness,
            });
        }
    }

    pub fn push_strikethrough(&mut self, bounds: LogicalRect, color: ColorU, thickness: f32) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Strikethrough {
                bounds,
                color,
                thickness,
            });
        }
    }

    pub fn push_overline(&mut self, bounds: LogicalRect, color: ColorU, thickness: f32) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Overline {
                bounds,
                color,
                thickness,
            });
        }
    }

    pub fn push_image(&mut self, bounds: LogicalRect, key: ImageKey) {
        self.push_item(DisplayListItem::Image { bounds, key });
    }
}

/// Main entry point for generating the display list.
pub fn generate_display_list<T: ParsedFontTrait + Sync + 'static>(
    ctx: &mut LayoutContext<T>,
    tree: &LayoutTree,
    calculated_positions: &BTreeMap<usize, LogicalPosition>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    scroll_ids: &BTreeMap<usize, u64>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &azul_core::resources::RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: azul_core::dom::DomId,
) -> Result<DisplayList> {
    debug_info!(ctx, "Starting display list generation");
    debug_info!(ctx, "Collecting stacking contexts from root node {}", tree.root);

    let positioned_tree = PositionedTree {
        tree,
        calculated_positions,
    };
    let mut generator = DisplayListGenerator::new(
        ctx,
        scroll_offsets,
        &positioned_tree,
        scroll_ids,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
    );
    let mut builder = DisplayListBuilder::new();

    // 1. Build a tree of stacking contexts, which defines the global paint order.
    let stacking_context_tree = generator.collect_stacking_contexts(tree.root)?;

    // 2. Traverse the stacking context tree to generate display items in the correct order.
    debug_info!(generator.ctx, "Generating display items from stacking context tree");
    generator.generate_for_stacking_context(&mut builder, &stacking_context_tree)?;

    let display_list = builder.build();
    debug_info!(generator.ctx, 
        "Generated display list with {} items",
        display_list.items.len()
    );
    Ok(display_list)
}

/// A helper struct that holds all necessary state and context for the generation process.
struct DisplayListGenerator<'a, 'b, T: ParsedFontTrait> {
    ctx: &'a mut LayoutContext<'b, T>,
    scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>,
    positioned_tree: &'a PositionedTree<'a>,
    scroll_ids: &'a BTreeMap<usize, u64>,
    gpu_value_cache: Option<&'a azul_core::gpu::GpuValueCache>,
    renderer_resources: &'a azul_core::resources::RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
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

impl<'a, 'b, T> DisplayListGenerator<'a, 'b, T>
where
    T: ParsedFontTrait + Sync + 'static,
{
    pub fn new(
        ctx: &'a mut LayoutContext<'b, T>,
        scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>,
        positioned_tree: &'a PositionedTree<'a>,
        scroll_ids: &'a BTreeMap<usize, u64>,
        gpu_value_cache: Option<&'a azul_core::gpu::GpuValueCache>,
        renderer_resources: &'a azul_core::resources::RendererResources,
        id_namespace: azul_core::resources::IdNamespace,
        dom_id: azul_core::dom::DomId,
    ) -> Self {
        Self {
            ctx,
            scroll_offsets,
            positioned_tree,
            scroll_ids,
            gpu_value_cache,
            renderer_resources,
            id_namespace,
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
        let Some(cached_layout) = &node.inline_layout_result else {
            return Ok(());
        };
        let layout = &cached_layout.layout;

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
            .calculated_positions
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
    fn collect_stacking_contexts(&mut self, node_index: usize) -> Result<StackingContext> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;
        let z_index = get_z_index(self.ctx.styled_dom, node.dom_node_id);

        if let Some(dom_id) = node.dom_node_id {
            let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            debug_info!(self.ctx, 
                "Collecting stacking context for node {} ({:?}), z-index={}",
                node_index,
                node_type.get_node_type(),
                z_index
            );
        }

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
        &mut self,
        builder: &mut DisplayListBuilder,
        context: &StackingContext,
    ) -> Result<()> {
        // Before painting the node, check if it establishes a new clip or scroll frame.
        let node = self
            .positioned_tree
            .tree
            .get(context.node_index)
            .ok_or(LayoutError::InvalidTree)?;
        
        if let Some(dom_id) = node.dom_node_id {
            let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            debug_info!(self.ctx, 
                "Painting stacking context for node {} ({:?}), z-index={}, {} child contexts, {} in-flow children",
                context.node_index,
                node_type.get_node_type(),
                context.z_index,
                context.child_contexts.len(),
                context.in_flow_children.len()
            );
        }
        
        let did_push_clip_or_scroll = self.push_node_clips(builder, context.node_index, node)?;

        // Push a stacking context for WebRender
        // Get the node's bounds for the stacking context
        let node_pos = self
            .positioned_tree
            .calculated_positions
            .get(&context.node_index)
            .copied()
            .unwrap_or_default();
        let node_size = node.used_size.unwrap_or(LogicalSize {
            width: 0.0,
            height: 0.0,
        });
        let node_bounds = LogicalRect {
            origin: node_pos,
            size: node_size,
        };
        builder.push_stacking_context(context.z_index, node_bounds);

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

        // Pop the stacking context for WebRender
        builder.pop_stacking_context();

        // After painting the node and all its descendants, pop any contexts it pushed.
        if did_push_clip_or_scroll {
            self.pop_node_clips(builder, node)?;
        }

        Ok(())
    }

    /// Paints the content and non-stacking-context children.
    fn paint_in_flow_descendants(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
        children_indices: &[usize],
    ) -> Result<()> {
        // NOTE: We do NOT paint the node's background here - that was already done by
        // generate_for_stacking_context! Only paint selection, cursor, and content for the
        // current node

        // 2. Paint selection highlights and the text cursor if applicable.
        self.paint_selection_and_cursor(builder, node_index)?;

        // 3. Paint the node's own content (text, images, hit-test areas).
        self.paint_node_content(builder, node_index)?;

        // 4. Recursively paint the in-flow children in correct CSS painting order:
        //    - First: Non-float block-level children
        //    - Then: Float children (so they appear on top)
        //    - Finally: Inline-level children (though typically handled above in paint_node_content)
        
        // Separate children into floats and non-floats
        let mut non_float_children = Vec::new();
        let mut float_children = Vec::new();
        
        for &child_index in children_indices {
            let child_node = self
                .positioned_tree
                .tree
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;
            
            // Check if this child is a float
            let is_float = if let Some(dom_id) = child_node.dom_node_id {
                use crate::solver3::getters::get_float;
                let styled_node_state = self.get_styled_node_state(dom_id);
                let float_value = get_float(self.ctx.styled_dom, dom_id, &styled_node_state);
                !matches!(float_value.unwrap_or_default(), azul_css::props::layout::LayoutFloat::None)
            } else {
                false
            };
            
            if is_float {
                float_children.push(child_index);
            } else {
                non_float_children.push(child_index);
            }
        }
        
        // Paint non-float children first
        for child_index in non_float_children {
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
        
        // Paint float children AFTER non-floats (so they appear on top)
        for child_index in float_children {
            let child_node = self
                .positioned_tree
                .tree
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;

            let did_push_clip = self.push_node_clips(builder, child_index, child_node)?;
            self.paint_node_background_and_border(builder, child_index)?;
            self.paint_in_flow_descendants(builder, child_index, &child_node.children)?;

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
        node: &LayoutNode,
    ) -> Result<bool> {
        let Some(dom_id) = node.dom_node_id else {
            return Ok(false);
        };

        let styled_node_state = self.get_styled_node_state(dom_id);

        let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
        let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);
        
        let paint_rect = self.get_paint_rect(node_index).unwrap_or_default();
        let element_size = PhysicalSizeImport {
            width: paint_rect.size.width,
            height: paint_rect.size.height,
        };
        let border_radius = get_border_radius(self.ctx.styled_dom, dom_id, &styled_node_state, element_size, self.ctx.viewport_size);

        let needs_clip = overflow_x.is_clipped() || overflow_y.is_clipped();

        if !needs_clip {
            return Ok(false);
        }

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
            // Always a scroll frame if overflow: scroll
            let scroll_id = self.scroll_ids.get(&node_index).copied().unwrap_or(0);
            let content_size = get_scroll_content_size(node);
            builder.push_scroll_frame(clip_rect, content_size, scroll_id);
        } else if overflow_x.is_auto_overflow() || overflow_y.is_auto_overflow() {
            // overflow: auto - check if content actually overflows
            let content_size = get_scroll_content_size(node);
            let container_size = LogicalSize {
                width: clip_rect.size.width,
                height: clip_rect.size.height,
            };

            let overflows_x = content_size.width > container_size.width;
            let overflows_y = content_size.height > container_size.height;

            // If overflow: auto and content overflows, treat as scroll frame
            if (overflow_x.is_auto_overflow() && overflows_x)
                || (overflow_y.is_auto_overflow() && overflows_y)
            {
                let scroll_id = self.scroll_ids.get(&node_index).copied().unwrap_or(0);
                builder.push_scroll_frame(clip_rect, content_size, scroll_id);
            } else {
                // No overflow, just clip
                builder.push_clip(clip_rect, border_radius);
            }
        } else {
            // It's a simple clip
            builder.push_clip(clip_rect, border_radius);
        }

        Ok(true)
    }

    /// Pops any clip/scroll commands associated with a node.
    fn pop_node_clips(&self, builder: &mut DisplayListBuilder, node: &LayoutNode) -> Result<()> {
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };

        let styled_node_state = self.get_styled_node_state(dom_id);
        let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
        let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);
        
        let paint_rect = self
            .get_paint_rect(
                self.positioned_tree
                    .tree
                    .nodes
                    .iter()
                    .position(|n| n.dom_node_id == Some(dom_id))
                    .unwrap_or(0),
            )
            .unwrap_or_default();
        
        let element_size = PhysicalSizeImport {
            width: paint_rect.size.width,
            height: paint_rect.size.height,
        };
        let border_radius = get_border_radius(self.ctx.styled_dom, dom_id, &styled_node_state, element_size, self.ctx.viewport_size);

        let needs_clip = overflow_x.is_hidden_or_clip()
            || overflow_y.is_hidden_or_clip()
            || !border_radius.is_zero();

        if needs_clip {
            if overflow_x.is_scroll_explicit() || overflow_y.is_scroll_explicit() {
                // Always pop scroll frame for overflow: scroll
                builder.pop_scroll_frame();
            } else if overflow_x.is_auto_overflow() || overflow_y.is_auto_overflow() {
                // For overflow: auto, check if we actually created a scroll frame
                // by checking if content overflows
                let content_size = get_scroll_content_size(node);
                let paint_rect = self
                    .get_paint_rect(
                        self.positioned_tree
                            .tree
                            .nodes
                            .iter()
                            .position(|n| n.dom_node_id == Some(dom_id))
                            .unwrap_or(0),
                    )
                    .unwrap_or_default();

                let border = &node.box_props.border;
                let container_size = LogicalSize {
                    width: (paint_rect.size.width - border.left - border.right).max(0.0),
                    height: (paint_rect.size.height - border.top - border.bottom).max(0.0),
                };

                let overflows_x = content_size.width > container_size.width;
                let overflows_y = content_size.height > container_size.height;

                if (overflow_x.is_auto_overflow() && overflows_x)
                    || (overflow_y.is_auto_overflow() && overflows_y)
                {
                    builder.pop_scroll_frame();
                } else {
                    builder.pop_clip();
                }
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
            .calculated_positions
            .get(&node_index)
            .copied()
            .unwrap_or_default();
        let size = node.used_size.unwrap_or_default();

        // Apply scroll offset from parent if present
        let scroll_offset = node.parent
            .and_then(|parent_idx| self.positioned_tree.tree.get(parent_idx))
            .and_then(|p| p.dom_node_id)
            .and_then(|parent_dom_id| self.scroll_offsets.get(&parent_dom_id));
        
        if let Some(scroll) = scroll_offset {
            pos.x -= scroll.children_rect.origin.x;
            pos.y -= scroll.children_rect.origin.y;
        }
        
        Some(LogicalRect::new(pos, size))
    }

    /// Emits drawing commands for the background and border of a single node.
    fn paint_node_background_and_border(
        &mut self,
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

        // Set current node for node mapping (for pagination break properties)
        builder.set_current_node(node.dom_node_id);

        // Skip inline-blocks - they are rendered by text3 in paint_inline_content
        // Inline-blocks participate in inline formatting context and their backgrounds
        // must be positioned by the text layout engine, not the block layout engine
        if let Some(dom_id) = node.dom_node_id {
            use azul_css::props::layout::LayoutDisplay;
            let styled_node_state = self.get_styled_node_state(dom_id);
            let display = self.ctx.styled_dom.css_property_cache.ptr
                .get_display(&self.ctx.styled_dom.node_data.as_container()[dom_id], &dom_id, &styled_node_state)
                .and_then(|v| v.get_property().cloned())
                .unwrap_or(LayoutDisplay::Inline);
            
            if display == LayoutDisplay::InlineBlock {
                // text3 will handle this via InlineShape
                return Ok(());
            }
        }

        // CSS 2.2 Section 17.5.1: Tables in the visual formatting model
        // Tables have a special 6-layer background painting order
        use azul_core::dom::FormattingContext;
        if matches!(node.formatting_context, FormattingContext::Table) {
            debug_info!(self.ctx, 
                "Painting table backgrounds/borders for node {} at {:?}",
                node_index, paint_rect
            );
            // Delegate to specialized table painting function
            return self.paint_table_items(builder, node_index);
        }

        let border_radius = if let Some(dom_id) = node.dom_node_id {
            let styled_node_state = self.get_styled_node_state(dom_id);
            let bg_color = get_background_color(self.ctx.styled_dom, dom_id, &styled_node_state);
            let border_info = get_border_info(self.ctx.styled_dom, dom_id, &styled_node_state);

            let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            debug_info!(self.ctx, 
                "Painting background/border for node {} ({:?}) at {:?}, bg_color={:?}",
                node_index,
                node_type.get_node_type(),
                paint_rect,
                bg_color
            );

            // Get both versions: simple BorderRadius for rect clipping and StyleBorderRadius for
            // border rendering
            let element_size = PhysicalSizeImport {
                width: paint_rect.size.width,
                height: paint_rect.size.height,
            };
            let simple_border_radius =
                get_border_radius(self.ctx.styled_dom, dom_id, &styled_node_state, element_size, self.ctx.viewport_size);
            let style_border_radius =
                get_style_border_radius(self.ctx.styled_dom, dom_id, &styled_node_state);

            builder.push_rect(paint_rect, bg_color, simple_border_radius);
            builder.push_border(
                paint_rect,
                border_info.widths,
                border_info.colors,
                border_info.styles,
                style_border_radius,
            );
            
            simple_border_radius
        } else {
            BorderRadius::default()
        };

        Ok(())
    }

    /// CSS 2.2 Section 17.5.1: Table background painting in 6 layers
    /// 
    /// Implements the CSS 2.2 specification for table background painting order.
    /// Unlike regular block elements, tables paint backgrounds in layers from back to front:
    /// 
    /// 1. Table background (lowest layer)
    /// 2. Column group backgrounds
    /// 3. Column backgrounds  
    /// 4. Row group backgrounds
    /// 5. Row backgrounds
    /// 6. Cell backgrounds (topmost layer)
    /// 
    /// Then borders are painted (respecting border-collapse mode).
    /// Finally, cell content is painted on top of everything.
    /// 
    /// This function generates simple display list items (Rect, Border) in the correct
    /// CSS paint order, making WebRender integration trivial.
    fn paint_table_items(
        &self,
        builder: &mut DisplayListBuilder,
        table_index: usize,
    ) -> Result<()> {
        let table_node = self
            .positioned_tree
            .tree
            .get(table_index)
            .ok_or(LayoutError::InvalidTree)?;

        let Some(table_paint_rect) = self.get_paint_rect(table_index) else {
            return Ok(());
        };

        // Layer 1: Table background
        if let Some(dom_id) = table_node.dom_node_id {
            let styled_node_state = self.get_styled_node_state(dom_id);
            let bg_color = get_background_color(self.ctx.styled_dom, dom_id, &styled_node_state);
            let element_size = PhysicalSizeImport {
                width: table_paint_rect.size.width,
                height: table_paint_rect.size.height,
            };
            let border_radius = get_border_radius(self.ctx.styled_dom, dom_id, &styled_node_state, element_size, self.ctx.viewport_size);
            
            builder.push_rect(table_paint_rect, bg_color, border_radius);
        }

        // Traverse table children to paint layers 2-6
        use azul_core::dom::FormattingContext;
        
        // Layer 2: Column group backgrounds
        // Layer 3: Column backgrounds (columns are children of column groups)
        for &child_idx in &table_node.children {
            let child_node = self.positioned_tree.tree.get(child_idx);
            if let Some(node) = child_node {
                if matches!(node.formatting_context, FormattingContext::TableColumnGroup) {
                    // Paint column group background
                    self.paint_element_background(builder, child_idx)?;
                    
                    // Paint backgrounds of individual columns within this group
                    for &col_idx in &node.children {
                        self.paint_element_background(builder, col_idx)?;
                    }
                }
            }
        }

        // Layer 4: Row group backgrounds (tbody, thead, tfoot)
        // Layer 5: Row backgrounds
        // Layer 6: Cell backgrounds
        for &child_idx in &table_node.children {
            let child_node = self.positioned_tree.tree.get(child_idx);
            if let Some(node) = child_node {
                match node.formatting_context {
                    FormattingContext::TableRowGroup => {
                        // Paint row group background
                        self.paint_element_background(builder, child_idx)?;
                        
                        // Paint rows within this group
                        for &row_idx in &node.children {
                            self.paint_table_row_and_cells(builder, row_idx)?;
                        }
                    }
                    FormattingContext::TableRow => {
                        // Direct row child (no row group wrapper)
                        self.paint_table_row_and_cells(builder, child_idx)?;
                    }
                    _ => {}
                }
            }
        }

        // Borders are painted separately after all backgrounds
        // This is handled by the normal rendering flow for each element
        // TODO: Implement border-collapse conflict resolution using BorderInfo::resolve_conflict()

        Ok(())
    }

    /// Helper function to paint a table row's background and then its cells' backgrounds
    /// Layer 5: Row background
    /// Layer 6: Cell backgrounds (painted after row, so they appear on top)
    fn paint_table_row_and_cells(
        &self,
        builder: &mut DisplayListBuilder,
        row_idx: usize,
    ) -> Result<()> {
        
        // Layer 5: Paint row background
        self.paint_element_background(builder, row_idx)?;
        
        // Layer 6: Paint cell backgrounds (topmost layer)
        let row_node = self.positioned_tree.tree.get(row_idx);
        if let Some(node) = row_node {
            use azul_core::dom::FormattingContext;
            for &cell_idx in &node.children {
                self.paint_element_background(builder, cell_idx)?;
            }
        }
        
        Ok(())
    }

    /// Helper function to paint an element's background (used for all table elements)
    /// Reads background-color and border-radius from CSS properties and emits push_rect()
    fn paint_element_background(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };
        
        let Some(node) = self.positioned_tree.tree.get(node_index) else {
            return Ok(());
        };
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };
        
        let styled_node_state = self.get_styled_node_state(dom_id);
        let bg_color = get_background_color(self.ctx.styled_dom, dom_id, &styled_node_state);
        
        // Only paint if background color has alpha > 0 (optimization)
        if bg_color.a == 0 {
            return Ok(());
        }
        
        let element_size = PhysicalSizeImport {
            width: paint_rect.size.width,
            height: paint_rect.size.height,
        };
        let border_radius = get_border_radius(self.ctx.styled_dom, dom_id, &styled_node_state, element_size, self.ctx.viewport_size);
        
        builder.push_rect(paint_rect, bg_color, border_radius);
        
        Ok(())
    }

    /// Emits drawing commands for the foreground content, including hit-test areas and scrollbars.
    fn paint_node_content(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;

        // Set current node for node mapping (for pagination break properties)
        builder.set_current_node(node.dom_node_id);

        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };

        // Add a hit-test area for this node if it's interactive.
        if let Some(tag_id) = get_tag_id(self.ctx.styled_dom, node.dom_node_id) {
            builder.push_hit_test_area(paint_rect, tag_id);
        }

        // Paint the node's visible content.
        if let Some(cached_layout) = &node.inline_layout_result {
            let inline_layout = &cached_layout.layout;
            if let Some(dom_id) = node.dom_node_id {
                let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
                debug_info!(self.ctx, 
                    "Painting inline content for node {} ({:?}) at {:?}, {} layout items",
                    node_index,
                    node_type.get_node_type(),
                    paint_rect,
                    inline_layout.items.len()
                );
            }
            
            // paint_rect is the border-box, but inline layout positions are relative to content-box.
            // Use type-safe conversion to make this clear and avoid manual calculations.
            let border_box = BorderBoxRect(paint_rect);
            let content_box = border_box.to_content_box(&node.box_props.padding, &node.box_props.border);
            
            self.paint_inline_content(builder, content_box.rect(), inline_layout)?;;
        } else if let Some(dom_id) = node.dom_node_id {
            // This node might be a simple replaced element, like an <img> tag.
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            if let NodeType::Image(image_data) = node_data.get_node_type() {
                debug_info!(self.ctx, 
                    "Painting image for node {} at {:?}",
                    node_index, paint_rect
                );
                let image_key = get_image_key_for_src(&image_data.get_hash(), self.id_namespace);
                builder.push_image(paint_rect, image_key);
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
        layout: &UnifiedLayout,
    ) -> Result<()> {
        // TODO: This will always paint images over the glyphs
        // TODO: Handle z-index within inline content (e.g. background images)
        // TODO: Handle text decorations (underline, strikethrough, etc.)
        // TODO: Handle text shadows
        // TODO: Handle text overflowing (based on container_rect and overflow behavior)
        
        // Push the TextLayout item FIRST, containing the full UnifiedLayout for PDF/accessibility
        // This provides complete metadata including original text and glyph-to-unicode mapping
        builder.push_text_layout(
            Arc::new(layout.clone()) as Arc<dyn std::any::Any + Send + Sync>,
            container_rect,
            FontHash::from_hash(0), // Will be updated per glyph run
            12.0, // Default font size, will be updated per glyph run
            ColorU { r: 0, g: 0, b: 0, a: 255 }, // Default color
        );
        
        let glyph_runs = crate::text3::glyphs::get_glyph_runs_simple(layout);

        for (idx, glyph_run) in glyph_runs.iter().enumerate() {
            let clip_rect = container_rect; // Clip to the container rect
            
            // IMPORTANT: Inline background colors (e.g., <span style="background: yellow">)
            // are NOT rendered here via push_rect().
            // 
            // Reason: The PDF renderer processes DisplayListItem::TextLayout, which contains
            // the full UnifiedLayout. The renderer extracts glyph runs with their background_color
            // and renders backgrounds in a FIRST PASS, then text in a SECOND PASS.
            // 
            // If we called push_rect() here, it would:
            // 1. Add Rect items AFTER the TextLayout item in the display list
            // 2. The PDF renderer would render TextLayout (backgrounds + text)
            // 3. Then render the Rect items ON TOP of the text
            // 4. Result: Text hidden behind backgrounds (wrong z-order)
            // 
            // The background_color is stored in StyleProperties -> ShapedGlyph -> PdfGlyphRun
            // and the PDF renderer handles it correctly via get_glyph_runs_pdf().
            
            // Store only the font hash in the display list to keep it lean
            builder.push_text_run(
                glyph_run.glyphs.clone(),
                FontHash::from_hash(glyph_run.font_hash),
                glyph_run.font_size_px,
                glyph_run.color,
                clip_rect,
            );

            // Render text decorations if present OR if this is IME composition preview
            let needs_underline = glyph_run.text_decoration.underline || glyph_run.is_ime_preview;
            let needs_strikethrough = glyph_run.text_decoration.strikethrough;
            let needs_overline = glyph_run.text_decoration.overline;

            if needs_underline || needs_strikethrough || needs_overline {
                // Calculate the bounding box for this glyph run
                if let (Some(first_glyph), Some(last_glyph)) =
                    (glyph_run.glyphs.first(), glyph_run.glyphs.last())
                {
                    let decoration_start_x = container_rect.origin.x + first_glyph.point.x;
                    let decoration_end_x = container_rect.origin.x + last_glyph.point.x;
                    let decoration_width = decoration_end_x - decoration_start_x;

                    // Use font metrics to determine decoration positions
                    // Standard ratios based on CSS specification
                    let font_size = glyph_run.font_size_px;
                    let thickness = (font_size * 0.08).max(1.0); // ~8% of font size, min 1px

                    // Baseline is at glyph.point.y
                    let baseline_y = container_rect.origin.y + first_glyph.point.y;

                    if needs_underline {
                        // Underline is typically 10-15% below baseline
                        // IME composition always gets underlined
                        let underline_y = baseline_y + (font_size * 0.12);
                        let underline_bounds = LogicalRect::new(
                            LogicalPosition::new(decoration_start_x, underline_y),
                            LogicalSize::new(decoration_width, thickness),
                        );
                        builder.push_underline(underline_bounds, glyph_run.color, thickness);
                    }

                    if needs_strikethrough {
                        // Strikethrough is typically 40% above baseline (middle of x-height)
                        let strikethrough_y = baseline_y - (font_size * 0.3);
                        let strikethrough_bounds = LogicalRect::new(
                            LogicalPosition::new(decoration_start_x, strikethrough_y),
                            LogicalSize::new(decoration_width, thickness),
                        );
                        builder.push_strikethrough(
                            strikethrough_bounds,
                            glyph_run.color,
                            thickness,
                        );
                    }

                    if needs_overline {
                        // Overline is typically at cap-height (75% above baseline)
                        let overline_y = baseline_y - (font_size * 0.85);
                        let overline_bounds = LogicalRect::new(
                            LogicalPosition::new(decoration_start_x, overline_y),
                            LogicalSize::new(decoration_width, thickness),
                        );
                        builder.push_overline(overline_bounds, glyph_run.color, thickness);
                    }
                }
            }
        }

        // Render inline objects (images, shapes/inline-blocks, etc.)
        // These are positioned by the text3 engine and need to be rendered at their calculated positions
        for positioned_item in &layout.items {
            self.paint_inline_object(builder, container_rect.origin, positioned_item)?;
        }
        Ok(())
    }

    /// Paints a single inline object (image, shape, or inline-block)
    fn paint_inline_object(
        &self,
        builder: &mut DisplayListBuilder,
        base_pos: LogicalPosition,
        positioned_item: &PositionedItem,
    ) -> Result<()> {
        let ShapedItem::Object { content, bounds, .. } = &positioned_item.item else {
            // Other item types (e.g., breaks) don't produce painted output.
            return Ok(());
        };

        // Calculate the absolute position of this object
        // positioned_item.position is relative to the container
        let object_bounds = LogicalRect::new(
            LogicalPosition::new(
                base_pos.x + positioned_item.position.x,
                base_pos.y + positioned_item.position.y,
            ),
            LogicalSize::new(bounds.width, bounds.height),
        );
        
        match content {
            InlineContent::Image(image) => {
                if let Some(image_key) =
                    get_image_key_for_image_source(&image.source, self.id_namespace)
                {
                    builder.push_image(object_bounds, image_key);
                }
            }
            InlineContent::Shape(shape) => {
                self.paint_inline_shape(builder, object_bounds, shape, bounds)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Paints an inline shape (inline-block background)
    fn paint_inline_shape(
        &self,
        builder: &mut DisplayListBuilder,
        object_bounds: LogicalRect,
        shape: &InlineShape,
        bounds: &crate::text3::cache::Rect,
    ) -> Result<()> {
        // Render inline-block backgrounds using their CSS styling
        // The text3 engine positions these correctly in the inline flow
        let Some(node_id) = shape.source_node_id else {
            return Ok(());
        };
        
        let styled_node_state = &self.ctx.styled_dom.styled_nodes.as_container()[node_id].state;
        let bg_color = get_background_color(
            self.ctx.styled_dom,
            node_id,
            styled_node_state,
        );
        
        // Only render if there's a visible background
        if bg_color.a == 0 {
            return Ok(());
        }
        
        let element_size = PhysicalSizeImport {
            width: bounds.width,
            height: bounds.height,
        };
        let border_radius = get_border_radius(
            self.ctx.styled_dom,
            node_id,
            styled_node_state,
            element_size,
            self.ctx.viewport_size,
        );
        
        builder.push_rect(object_bounds, bg_color, border_radius);
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
            let opacity = self.ctx.styled_dom.css_property_cache.ptr
                .get_opacity(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property())
                .map(|v| v.inner.normalized())
                .unwrap_or(1.0);
            
            if opacity < 1.0 {
                return true;
            }

            // Transform != none
            let has_transform = self.ctx.styled_dom.css_property_cache.ptr
                .get_transform(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property())
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            
            if has_transform {
                return true;
            }
        }

        false
    }
}

/// Helper struct to pass layout results to the generator.
pub struct PositionedTree<'a> {
    pub tree: &'a LayoutTree,
    pub calculated_positions: &'a BTreeMap<usize, LogicalPosition>,
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

/// Calculates the actual content size of a node, including all children and text.
/// This is used to determine if scrollbars should appear for overflow: auto.
fn get_scroll_content_size(node: &LayoutNode) -> LogicalSize {
    // Start with the node's own size
    let mut content_size = node.used_size.unwrap_or_default();

    // If this node has text layout, calculate the bounds of all text items
    if let Some(ref cached_layout) = node.inline_layout_result {
        let text_layout = &cached_layout.layout;
        // Find the maximum extent of all positioned items
        let mut max_x: f32 = 0.0;
        let mut max_y: f32 = 0.0;

        for positioned_item in &text_layout.items {
            let item_bounds = positioned_item.item.bounds();
            let item_right = positioned_item.position.x + item_bounds.width;
            let item_bottom = positioned_item.position.y + item_bounds.height;

            max_x = max_x.max(item_right);
            max_y = max_y.max(item_bottom);
        }

        // Use the maximum extent as content size if it's larger
        content_size.width = content_size.width.max(max_x);
        content_size.height = content_size.height.max(max_y);
    }

    // TODO: Also check children positions to get max content bounds
    // For now, this handles the most common case (text overflowing)

    content_size
}

fn get_tag_id(dom: &StyledDom, id: Option<NodeId>) -> Option<TagId> {
    id.map(|i| i.index() as u64)
}

fn get_image_key_for_src(src: &ImageRefHash, namespace: IdNamespace) -> ImageKey {
    azul_core::resources::image_ref_hash_to_image_key(*src, namespace)
}

fn get_image_key_for_image_source(
    _source: &ImageSource,
    _namespace: IdNamespace,
) -> Option<ImageKey> {
    // TODO: ImageSource needs to be extended to contain ImageRef/ImageRefHash
    // For now, inline images are not yet supported
    None
}

// Phase 3: Per-Page Display List Generation

/// Generate display lists for paged layout, one per page.
/// 
/// This function groups layout nodes by their page_index and generates
/// a separate DisplayList for each page. Items are offset to page-relative
/// coordinates.
/// 
/// # Arguments
/// * `ctx` - The layout context
/// * `tree` - The layout tree with page_index assigned to nodes
/// * `calculated_positions` - Absolute positions of all nodes
/// * `page_content_height` - Height of each page's content area
/// * Other arguments same as generate_display_list()
/// 
/// # Returns
/// A vector of DisplayLists, one per page.
pub fn generate_display_lists_paged<T: ParsedFontTrait + Sync + 'static>(
    ctx: &mut LayoutContext<T>,
    tree: &LayoutTree,
    calculated_positions: &BTreeMap<usize, LogicalPosition>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    scroll_ids: &BTreeMap<usize, u64>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &azul_core::resources::RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: azul_core::dom::DomId,
    page_content_height: f32,
) -> Result<Vec<DisplayList>> {
    // First, generate a single display list with all items
    let full_display_list = generate_display_list(
        ctx,
        tree,
        calculated_positions,
        scroll_offsets,
        scroll_ids,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
    )?;
    
    // If page_content_height is invalid, return single page
    if page_content_height <= 0.0 || page_content_height >= f32::MAX {
        return Ok(vec![full_display_list]);
    }
    
    // Use simple slicer-based pagination for this legacy API
    // The new commitment-based pagination with CSS break properties
    // is used via layout_document_paged_with_config
    let config = SlicerConfig::simple(page_content_height);
    paginate_display_list_with_slicer(full_display_list, &config)
}

/// Get the bounds of a display list item, if it has spatial extent.
fn get_display_item_bounds(item: &DisplayListItem) -> Option<LogicalRect> {
    match item {
        DisplayListItem::Rect { bounds, .. } => Some(*bounds),
        DisplayListItem::SelectionRect { bounds, .. } => Some(*bounds),
        DisplayListItem::CursorRect { bounds, .. } => Some(*bounds),
        DisplayListItem::Border { bounds, .. } => Some(*bounds),
        DisplayListItem::TextLayout { bounds, .. } => Some(*bounds),
        DisplayListItem::Text { clip_rect, .. } => Some(*clip_rect),
        DisplayListItem::Underline { bounds, .. } => Some(*bounds),
        DisplayListItem::Strikethrough { bounds, .. } => Some(*bounds),
        DisplayListItem::Overline { bounds, .. } => Some(*bounds),
        DisplayListItem::Image { bounds, .. } => Some(*bounds),
        DisplayListItem::ScrollBar { bounds, .. } => Some(*bounds),
        DisplayListItem::PushClip { bounds, .. } => Some(*bounds),
        DisplayListItem::PushScrollFrame { clip_bounds, .. } => Some(*clip_bounds),
        DisplayListItem::HitTestArea { bounds, .. } => Some(*bounds),
        DisplayListItem::PushStackingContext { bounds, .. } => Some(*bounds),
        DisplayListItem::IFrame { bounds, .. } => Some(*bounds),
        _ => None,
    }
}

/// Clip a display list item to page bounds and offset to page-relative coordinates.
/// Returns None if the item is completely outside the page bounds.
fn clip_and_offset_display_item(
    item: &DisplayListItem,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    match item {
        DisplayListItem::Rect { bounds, color, border_radius } => {
            clip_rect_bounds(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::Rect {
                    bounds: clipped,
                    color: *color,
                    border_radius: *border_radius,
                }
            })
        }
        
        DisplayListItem::Border { bounds, widths, colors, styles, border_radius } => {
            let original_bounds = *bounds;
            clip_rect_bounds(*bounds, page_top, page_bottom).map(|clipped| {
                let mut new_widths = *widths;
                
                // Hide top border if we clipped the top
                if clipped.origin.y > 0.0 && original_bounds.origin.y < page_top {
                    new_widths.top = None;
                }
                
                // Hide bottom border if we clipped the bottom
                let original_bottom = original_bounds.origin.y + original_bounds.size.height;
                let clipped_bottom = clipped.origin.y + clipped.size.height;
                if original_bottom > page_bottom && clipped_bottom >= page_bottom - page_top - 1.0 {
                    new_widths.bottom = None;
                }
                
                DisplayListItem::Border {
                    bounds: clipped,
                    widths: new_widths,
                    colors: *colors,
                    styles: *styles,
                    border_radius: border_radius.clone(),
                }
            })
        }
        
        DisplayListItem::SelectionRect { bounds, border_radius, color } => {
            clip_rect_bounds(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::SelectionRect {
                    bounds: clipped,
                    border_radius: *border_radius,
                    color: *color,
                }
            })
        }
        
        DisplayListItem::CursorRect { bounds, color } => {
            clip_rect_bounds(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::CursorRect {
                    bounds: clipped,
                    color: *color,
                }
            })
        }
        
        DisplayListItem::Image { bounds, key } => {
            // Images: show if they overlap the page
            if bounds.origin.y < page_bottom && bounds.origin.y + bounds.size.height > page_top {
                clip_rect_bounds(*bounds, page_top, page_bottom).map(|clipped| {
                    DisplayListItem::Image {
                        bounds: clipped,
                        key: *key,
                    }
                })
            } else {
                None
            }
        }
        
        DisplayListItem::TextLayout { layout, bounds, font_hash, font_size_px, color } => {
            if !rect_intersects(bounds, page_top, page_bottom) {
                return None;
            }
            
            // Try to downcast the layout to UnifiedLayout and filter its items
            // This ensures that only text items within the current page bounds are included
            #[cfg(feature = "text_layout")]
            {
                if let Some(unified_layout) = layout.downcast_ref::<crate::text3::cache::UnifiedLayout>() {
                    // The bounds.origin.y is the position of this TextLayout block on the infinite canvas.
                    // Item positions within UnifiedLayout are relative to this origin.
                    // 
                    // For pagination:
                    // - page_top, page_bottom define the slice of the infinite canvas for this page
                    // - We filter items whose center falls within this page
                    // - We then offset item positions to be relative to page_top (so page starts at y=0)
                    
                    let layout_origin_y = bounds.origin.y;
                    let layout_origin_x = bounds.origin.x;
                    
                    // First pass: filter items that belong to this page
                    // An item belongs to this page if its center point falls within [page_top, page_bottom)
                    let filtered_items: Vec<_> = unified_layout.items.iter()
                        .filter(|item| {
                            let item_y_relative = item.position.y;
                            let item_height = item.item.bounds().height;
                            
                            // Absolute position on the infinite canvas
                            let item_y_absolute = layout_origin_y + item_y_relative;
                            
                            // Center-point decision for page assignment
                            let item_center_y = item_y_absolute + (item_height / 2.0);
                            item_center_y >= page_top && item_center_y < page_bottom
                        })
                        .cloned()
                        .collect();
                    
                    if filtered_items.is_empty() {
                        return None;
                    }
                    
                    // Calculate the vertical offset needed to convert from canvas coords to page coords
                    // All Y positions need to subtract page_top to become page-relative
                    // Additionally, the TextLayout bounds.origin.y might be above or below page_top
                    
                    // The new bounds origin for this page's TextLayout:
                    // - X stays the same
                    // - Y becomes: max(0, bounds.origin.y - page_top)
                    //   If bounds.origin.y < page_top, some items are above the page top,
                    //   but they passed the filter (their center is on this page)
                    
                    let new_origin_y = (layout_origin_y - page_top).max(0.0);
                    
                    // Calculate the offset that items need to apply to their positions
                    // Items are currently relative to layout_origin_y on the canvas
                    // They need to become relative to new_origin_y on the page
                    // 
                    // Current absolute Y = layout_origin_y + item.position.y
                    // New page Y = current_absolute - page_top
                    // New relative = new_page_y - new_origin_y
                    //              = (layout_origin_y + item.position.y - page_top) - new_origin_y
                    
                    // Calculate bounds for the filtered items
                    let mut min_y = f32::MAX;
                    let mut max_y = f32::MIN;
                    let mut max_width = 0.0f32;
                    
                    let offset_items: Vec<_> = filtered_items.into_iter()
                        .map(|mut item| {
                            // Calculate the item's position on the page
                            let abs_y = layout_origin_y + item.position.y;
                            let page_y = abs_y - page_top;
                            
                            // The item position should be relative to new_origin_y
                            // Since new_origin_y is where the TextLayout block starts on the page
                            let new_item_y = page_y - new_origin_y;
                            
                            let item_bounds = item.item.bounds();
                            min_y = min_y.min(new_item_y);
                            max_y = max_y.max(new_item_y + item_bounds.height);
                            max_width = max_width.max(item.position.x + item_bounds.width);
                            
                            item.position.y = new_item_y;
                            item
                        })
                        .collect();
                    
                    let new_layout = crate::text3::cache::UnifiedLayout {
                        items: offset_items,
                        overflow: unified_layout.overflow.clone(),
                    };
                    
                    // Calculate the new bounds for this page's TextLayout
                    let new_bounds = LogicalRect {
                        origin: LogicalPosition {
                            x: layout_origin_x,
                            y: new_origin_y,
                        },
                        size: LogicalSize {
                            width: max_width.max(bounds.size.width),
                            height: (max_y - min_y.min(0.0)).max(0.0),
                        },
                    };
                    
                    return Some(DisplayListItem::TextLayout {
                        layout: Arc::new(new_layout) as Arc<dyn std::any::Any + Send + Sync>,
                        bounds: new_bounds,
                        font_hash: *font_hash,
                        font_size_px: *font_size_px,
                        color: *color,
                    });
                }
            }
            
            // Fallback: if not UnifiedLayout or text_layout feature disabled,
            // use simple bounds offset (legacy behavior)
            Some(DisplayListItem::TextLayout {
                layout: layout.clone(),
                bounds: offset_rect_y(*bounds, -page_top),
                font_hash: *font_hash,
                font_size_px: *font_size_px,
                color: *color,
            })
        }
        
        DisplayListItem::Text { glyphs, font_hash, font_size_px, color, clip_rect } => {
            if !rect_intersects(clip_rect, page_top, page_bottom) {
                return None;
            }
            
            // Filter glyphs to only those visible on this page
            // Use CENTER-POINT DECISION: glyph belongs to page containing its baseline
            // (g.point.y is the baseline position, which is effectively the glyph's center)
            // This prevents glyph duplication across page boundaries.
            let page_glyphs: Vec<_> = glyphs.iter()
                .filter(|g| g.point.y >= page_top && g.point.y < page_bottom)
                .map(|g| GlyphInstance {
                    index: g.index,
                    point: LogicalPosition {
                        x: g.point.x,
                        y: g.point.y - page_top,
                    },
                    size: g.size,
                })
                .collect();
            
            if page_glyphs.is_empty() {
                return None;
            }
            
            Some(DisplayListItem::Text {
                glyphs: page_glyphs,
                font_hash: *font_hash,
                font_size_px: *font_size_px,
                color: *color,
                clip_rect: offset_rect_y(*clip_rect, -page_top),
            })
        }
        
        DisplayListItem::Underline { bounds, color, thickness } => {
            clip_rect_bounds(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::Underline {
                    bounds: clipped,
                    color: *color,
                    thickness: *thickness,
                }
            })
        }
        
        DisplayListItem::Strikethrough { bounds, color, thickness } => {
            clip_rect_bounds(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::Strikethrough {
                    bounds: clipped,
                    color: *color,
                    thickness: *thickness,
                }
            })
        }
        
        DisplayListItem::Overline { bounds, color, thickness } => {
            clip_rect_bounds(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::Overline {
                    bounds: clipped,
                    color: *color,
                    thickness: *thickness,
                }
            })
        }
        
        DisplayListItem::ScrollBar { bounds, color, orientation, opacity_key, hit_id } => {
            clip_rect_bounds(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::ScrollBar {
                    bounds: clipped,
                    color: *color,
                    orientation: *orientation,
                    opacity_key: *opacity_key,
                    hit_id: *hit_id,
                }
            })
        }
        
        DisplayListItem::HitTestArea { bounds, tag } => {
            clip_rect_bounds(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::HitTestArea {
                    bounds: clipped,
                    tag: *tag,
                }
            })
        }
        
        // State management items - skip for now (would need proper per-page tracking)
        DisplayListItem::PushClip { .. } |
        DisplayListItem::PopClip |
        DisplayListItem::PushScrollFrame { .. } |
        DisplayListItem::PopScrollFrame |
        DisplayListItem::PushStackingContext { .. } |
        DisplayListItem::PopStackingContext => None,
        
        DisplayListItem::IFrame { child_dom_id, bounds, clip_rect } => {
            clip_rect_bounds(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::IFrame {
                    child_dom_id: *child_dom_id,
                    bounds: clipped,
                    clip_rect: offset_rect_y(*clip_rect, -page_top),
                }
            })
        }
    }
}

/// Clip a rectangle to page bounds and offset to page-relative coordinates.
/// Returns None if the rectangle is completely outside the page.
fn clip_rect_bounds(bounds: LogicalRect, page_top: f32, page_bottom: f32) -> Option<LogicalRect> {
    let item_top = bounds.origin.y;
    let item_bottom = bounds.origin.y + bounds.size.height;
    
    // Check if completely outside page
    if item_bottom <= page_top || item_top >= page_bottom {
        return None;
    }
    
    // Calculate clipped bounds
    let clipped_top = item_top.max(page_top);
    let clipped_bottom = item_bottom.min(page_bottom);
    let clipped_height = clipped_bottom - clipped_top;
    
    // Offset to page-relative coordinates
    let page_relative_y = clipped_top - page_top;
    
    Some(LogicalRect {
        origin: LogicalPosition {
            x: bounds.origin.x,
            y: page_relative_y,
        },
        size: LogicalSize {
            width: bounds.size.width,
            height: clipped_height,
        },
    })
}

/// Check if a rectangle intersects the page bounds.
fn rect_intersects(bounds: &LogicalRect, page_top: f32, page_bottom: f32) -> bool {
    let item_top = bounds.origin.y;
    let item_bottom = bounds.origin.y + bounds.size.height;
    item_bottom > page_top && item_top < page_bottom
}

/// Offset a rectangle's Y coordinate.
fn offset_rect_y(bounds: LogicalRect, offset_y: f32) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition {
            x: bounds.origin.x,
            y: bounds.origin.y + offset_y,
        },
        size: bounds.size,
    }
}

// =============================================================================
// SLICER-BASED PAGINATION (Infinite Canvas with Clipping)
// =============================================================================
//
// This approach treats pages as "viewports" into a single infinite canvas:
// 1. Layout generates ONE display list on an infinite vertical strip
// 2. Each page is a clip rectangle that "views" a portion of that strip
// 3. Items that span page boundaries are clipped and appear on BOTH pages
//
// Benefits:
// - Backgrounds render correctly (clipped at page boundary, not duplicated)
// - No complex page assignment logic during layout
// - Simple mental model: pages are just views into continuous content

use crate::solver3::pagination::{
    HeaderFooterConfig, PageInfo, TableHeaderTracker, TableHeaderInfo, MarginBoxContent,
    is_forced_break, is_avoid_break,
};
use azul_css::props::layout::fragmentation::{PageBreak, BreakInside};

/// Configuration for the slicer-based pagination.
#[derive(Debug, Clone, Default)]
pub struct SlicerConfig {
    /// Height of each page's content area (excludes margins, headers, footers)
    pub page_content_height: f32,
    /// Height of "dead zone" between pages (for margins, headers, footers)
    /// This represents space that content should NOT overlap with
    pub page_gap: f32,
    /// Whether to clip items that span page boundaries (true) or push them to next page (false)
    pub allow_clipping: bool,
    /// Header and footer configuration
    pub header_footer: HeaderFooterConfig,
    /// Width of the page content area (for centering headers/footers)
    pub page_width: f32,
    /// Table headers that need repetition across pages
    pub table_headers: TableHeaderTracker,
}

impl SlicerConfig {
    /// Create a simple slicer config with no gaps between pages.
    pub fn simple(page_height: f32) -> Self {
        Self {
            page_content_height: page_height,
            page_gap: 0.0,
            allow_clipping: true,
            header_footer: HeaderFooterConfig::default(),
            page_width: 595.0, // Default A4 width in points
            table_headers: TableHeaderTracker::default(),
        }
    }
    
    /// Create a slicer config with margins/gaps between pages.
    pub fn with_gap(page_height: f32, gap: f32) -> Self {
        Self {
            page_content_height: page_height,
            page_gap: gap,
            allow_clipping: true,
            header_footer: HeaderFooterConfig::default(),
            page_width: 595.0,
            table_headers: TableHeaderTracker::default(),
        }
    }
    
    /// Add header/footer configuration.
    pub fn with_header_footer(mut self, config: HeaderFooterConfig) -> Self {
        self.header_footer = config;
        self
    }
    
    /// Set the page width (for header/footer positioning).
    pub fn with_page_width(mut self, width: f32) -> Self {
        self.page_width = width;
        self
    }
    
    /// Add table headers for repetition.
    pub fn with_table_headers(mut self, tracker: TableHeaderTracker) -> Self {
        self.table_headers = tracker;
        self
    }
    
    /// Register a single table header.
    pub fn register_table_header(&mut self, info: TableHeaderInfo) {
        self.table_headers.register_table_header(info);
    }
    
    /// The total height of a page "slot" including the gap.
    pub fn page_slot_height(&self) -> f32 {
        self.page_content_height + self.page_gap
    }
    
    /// Calculate which page a Y coordinate falls on.
    pub fn page_for_y(&self, y: f32) -> usize {
        if self.page_slot_height() <= 0.0 {
            return 0;
        }
        (y / self.page_slot_height()).floor() as usize
    }
    
    /// Get the Y range for a specific page (in infinite canvas coordinates).
    pub fn page_bounds(&self, page_index: usize) -> (f32, f32) {
        let start = page_index as f32 * self.page_slot_height();
        let end = start + self.page_content_height;
        (start, end)
    }
}

/// Paginate a display list using the slicer approach.
/// 
/// This treats pages as viewports into a single continuous layout.
/// Items that span page boundaries are clipped and appear on multiple pages.
/// Headers and footers are injected per-page based on configuration.
/// 
/// # Arguments
/// * `full_display_list` - The complete display list from layout
/// * `config` - Slicer configuration (page height, gaps, headers/footers, etc.)
/// 
/// # Returns
/// A vector of DisplayLists, one per page.
pub fn paginate_display_list_with_slicer(
    full_display_list: DisplayList,
    config: &SlicerConfig,
) -> Result<Vec<DisplayList>> {
    // Use default (no break properties) implementation
    paginate_display_list_with_slicer_and_breaks(
        full_display_list,
        config,
        |_| BreakProperties::default(),
    )
}

/// Paginate with CSS break property support.
/// 
/// This function calculates page boundaries based on CSS break-before, break-after,
/// and break-inside properties, then clips content to those boundaries.
/// 
/// **Key insight**: Items are NEVER shifted. Instead, page boundaries are adjusted
/// to honor break properties.
pub fn paginate_display_list_with_slicer_and_breaks<F>(
    full_display_list: DisplayList,
    config: &SlicerConfig,
    get_break_properties: F,
) -> Result<Vec<DisplayList>> 
where
    F: Fn(Option<azul_core::dom::NodeId>) -> BreakProperties,
{
    if config.page_content_height <= 0.0 || config.page_content_height >= f32::MAX {
        return Ok(vec![full_display_list]);
    }
    
    // Calculate base header/footer space (used for pages that show headers/footers)
    let base_header_space = if config.header_footer.show_header { 
        config.header_footer.header_height 
    } else { 
        0.0 
    };
    let base_footer_space = if config.header_footer.show_footer { 
        config.header_footer.footer_height 
    } else { 
        0.0 
    };
    
    // Calculate effective heights for different page types
    let normal_page_content_height = config.page_content_height - base_header_space - base_footer_space;
    let first_page_content_height = if config.header_footer.skip_first_page {
        // First page has full height when skipping headers/footers
        config.page_content_height
    } else {
        normal_page_content_height
    };
    
    // =========================================================================
    // STEP 1: Calculate page break positions based on CSS properties
    // =========================================================================
    // 
    // Instead of using regular intervals, we calculate where page breaks
    // should occur based on:
    // - break-before: always → force break before this item
    // - break-after: always → force break after this item
    // - break-inside: avoid → don't break inside this item (push to next page if needed)
    
    let page_breaks = calculate_page_break_positions(
        &full_display_list,
        first_page_content_height,
        normal_page_content_height,
        &get_break_properties,
    );
    
    let num_pages = page_breaks.len();
    
    // Create per-page display lists by slicing the master list
    let mut pages: Vec<DisplayList> = Vec::with_capacity(num_pages);
    
    for (page_idx, &(content_start_y, content_end_y)) in page_breaks.iter().enumerate() {
        // Generate page info for header/footer content
        let page_info = PageInfo::new(page_idx + 1, num_pages);
        
        // Calculate per-page header/footer space
        let skip_this_page = config.header_footer.skip_first_page && page_info.is_first;
        let header_space = if config.header_footer.show_header && !skip_this_page {
            config.header_footer.header_height
        } else {
            0.0
        };
        let footer_space = if config.header_footer.show_footer && !skip_this_page {
            config.header_footer.footer_height
        } else {
            0.0
        };
        
        let _ = footer_space; // Currently unused but reserved for future
        
        let mut page_items = Vec::new();
        let mut page_node_mapping = Vec::new();
        
        // 1. Add header if enabled
        if config.header_footer.show_header && !skip_this_page {
            let header_text = config.header_footer.header_text(page_info);
            if !header_text.is_empty() {
                let header_items = generate_text_display_items(
                    &header_text,
                    LogicalRect {
                        origin: LogicalPosition { x: 0.0, y: 0.0 },
                        size: LogicalSize { 
                            width: config.page_width, 
                            height: config.header_footer.header_height 
                        },
                    },
                    config.header_footer.font_size,
                    config.header_footer.text_color,
                    TextAlignment::Center,
                );
                for item in header_items {
                    page_items.push(item);
                    page_node_mapping.push(None);
                }
            }
        }
        
        // 2. Inject repeated table headers (if any)
        let repeated_headers = config.table_headers.get_repeated_headers_for_page(
            page_idx,
            content_start_y,
            content_end_y,
        );
        
        let mut thead_total_height = 0.0f32;
        for (y_offset_from_page_top, thead_items, thead_height) in repeated_headers {
            let thead_y = header_space + y_offset_from_page_top;
            for item in thead_items {
                let translated_item = offset_display_item_y(item, thead_y);
                page_items.push(translated_item);
                page_node_mapping.push(None);
            }
            thead_total_height = thead_total_height.max(thead_height);
        }
        
        // 3. Calculate content offset (after header and repeated table headers)
        let content_y_offset = header_space + thead_total_height;
        
        // 4. Slice and offset content items
        for (item_idx, item) in full_display_list.items.iter().enumerate() {
            if let Some(clipped_item) = clip_and_offset_display_item(item, content_start_y, content_end_y) {
                let final_item = if content_y_offset > 0.0 {
                    offset_display_item_y(&clipped_item, content_y_offset)
                } else {
                    clipped_item
                };
                page_items.push(final_item);
                let node_mapping = full_display_list.node_mapping.get(item_idx).copied().flatten();
                page_node_mapping.push(node_mapping);
            }
        }
        
        // 5. Add footer if enabled
        if config.header_footer.show_footer && !skip_this_page {
            let footer_text = config.header_footer.footer_text(page_info);
            if !footer_text.is_empty() {
                let footer_y = config.page_content_height - config.header_footer.footer_height;
                let footer_items = generate_text_display_items(
                    &footer_text,
                    LogicalRect {
                        origin: LogicalPosition { x: 0.0, y: footer_y },
                        size: LogicalSize { 
                            width: config.page_width, 
                            height: config.header_footer.footer_height 
                        },
                    },
                    config.header_footer.font_size,
                    config.header_footer.text_color,
                    TextAlignment::Center,
                );
                for item in footer_items {
                    page_items.push(item);
                    page_node_mapping.push(None);
                }
            }
        }
        
        pages.push(DisplayList {
            items: page_items,
            node_mapping: page_node_mapping,
        });
    }
    
    // Ensure at least one page
    if pages.is_empty() {
        pages.push(DisplayList::default());
    }
    
    Ok(pages)
}

/// Calculate page break positions using REGULAR INTERVALS.
/// 
/// Returns a vector of (start_y, end_y) tuples representing each page's content bounds.
/// 
/// **IMPORTANT**: CSS break properties (break-before, break-after, break-inside) are 
/// currently NOT supported because they require shifting items, not just adjusting 
/// page boundaries. The slicer model assumes items stay at their original canvas 
/// positions. See PAGINATION_DEBUG_STATE.md for details.
/// 
/// TODO: To properly support CSS break properties, we need a commitment-based approach
/// that actually moves items when break-before: always is encountered.
fn calculate_page_break_positions<F>(
    display_list: &DisplayList,
    first_page_height: f32,
    normal_page_height: f32,
    _get_break_properties: &F,  // Currently unused - see note above
) -> Vec<(f32, f32)>
where
    F: Fn(Option<azul_core::dom::NodeId>) -> BreakProperties,
{
    let total_height = calculate_display_list_height(display_list);
    
    if total_height <= 0.0 || first_page_height <= 0.0 {
        return vec![(0.0, total_height.max(first_page_height))];
    }
    
    // Use simple regular intervals for page breaks
    // This ensures items at Y=X are correctly placed on page floor(X / page_height)
    let mut page_breaks: Vec<(f32, f32)> = Vec::new();
    let mut y = 0.0f32;
    let mut current_page_height = first_page_height;
    
    while y < total_height {
        let page_end = (y + current_page_height).min(total_height);
        page_breaks.push((y, page_end));
        y = page_end;
        current_page_height = normal_page_height;
    }
    
    // Ensure at least one page
    if page_breaks.is_empty() {
        page_breaks.push((0.0, total_height.max(first_page_height)));
    }
    
    page_breaks
}

/// Text alignment for generated header/footer text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

/// Helper to offset all Y coordinates of a display item.
fn offset_display_item_y(item: &DisplayListItem, y_offset: f32) -> DisplayListItem {
    if y_offset == 0.0 {
        return item.clone();
    }
    
    match item {
        DisplayListItem::Rect { bounds, color, border_radius } => {
            DisplayListItem::Rect {
                bounds: offset_rect_y(*bounds, y_offset),
                color: *color,
                border_radius: *border_radius,
            }
        }
        DisplayListItem::Border { bounds, widths, colors, styles, border_radius } => {
            DisplayListItem::Border {
                bounds: offset_rect_y(*bounds, y_offset),
                widths: widths.clone(),
                colors: *colors,
                styles: *styles,
                border_radius: border_radius.clone(),
            }
        }
        DisplayListItem::Text { glyphs, font_hash, font_size_px, color, clip_rect } => {
            let offset_glyphs: Vec<GlyphInstance> = glyphs
                .iter()
                .map(|g| GlyphInstance {
                    index: g.index,
                    point: LogicalPosition {
                        x: g.point.x,
                        y: g.point.y + y_offset,
                    },
                    size: g.size,
                })
                .collect();
            DisplayListItem::Text {
                glyphs: offset_glyphs,
                font_hash: *font_hash,
                font_size_px: *font_size_px,
                color: *color,
                clip_rect: offset_rect_y(*clip_rect, y_offset),
            }
        }
        DisplayListItem::TextLayout { layout, bounds, font_hash, font_size_px, color } => {
            DisplayListItem::TextLayout {
                layout: layout.clone(),
                bounds: offset_rect_y(*bounds, y_offset),
                font_hash: *font_hash,
                font_size_px: *font_size_px,
                color: *color,
            }
        }
        DisplayListItem::Image { bounds, key } => {
            DisplayListItem::Image {
                bounds: offset_rect_y(*bounds, y_offset),
                key: *key,
            }
        }
        // Pass through other items with their bounds offset
        DisplayListItem::SelectionRect { bounds, border_radius, color } => {
            DisplayListItem::SelectionRect {
                bounds: offset_rect_y(*bounds, y_offset),
                border_radius: *border_radius,
                color: *color,
            }
        }
        DisplayListItem::CursorRect { bounds, color } => {
            DisplayListItem::CursorRect {
                bounds: offset_rect_y(*bounds, y_offset),
                color: *color,
            }
        }
        DisplayListItem::Underline { bounds, color, thickness } => {
            DisplayListItem::Underline {
                bounds: offset_rect_y(*bounds, y_offset),
                color: *color,
                thickness: *thickness,
            }
        }
        DisplayListItem::Strikethrough { bounds, color, thickness } => {
            DisplayListItem::Strikethrough {
                bounds: offset_rect_y(*bounds, y_offset),
                color: *color,
                thickness: *thickness,
            }
        }
        DisplayListItem::Overline { bounds, color, thickness } => {
            DisplayListItem::Overline {
                bounds: offset_rect_y(*bounds, y_offset),
                color: *color,
                thickness: *thickness,
            }
        }
        DisplayListItem::ScrollBar { bounds, color, orientation, opacity_key, hit_id } => {
            DisplayListItem::ScrollBar {
                bounds: offset_rect_y(*bounds, y_offset),
                color: *color,
                orientation: *orientation,
                opacity_key: *opacity_key,
                hit_id: *hit_id,
            }
        }
        DisplayListItem::HitTestArea { bounds, tag } => {
            DisplayListItem::HitTestArea {
                bounds: offset_rect_y(*bounds, y_offset),
                tag: *tag,
            }
        }
        DisplayListItem::PushClip { bounds, border_radius } => {
            DisplayListItem::PushClip {
                bounds: offset_rect_y(*bounds, y_offset),
                border_radius: *border_radius,
            }
        }
        DisplayListItem::PushScrollFrame { clip_bounds, content_size, scroll_id } => {
            DisplayListItem::PushScrollFrame {
                clip_bounds: offset_rect_y(*clip_bounds, y_offset),
                content_size: *content_size,
                scroll_id: *scroll_id,
            }
        }
        DisplayListItem::PushStackingContext { bounds, z_index } => {
            DisplayListItem::PushStackingContext {
                bounds: offset_rect_y(*bounds, y_offset),
                z_index: *z_index,
            }
        }
        DisplayListItem::IFrame { child_dom_id, bounds, clip_rect } => {
            DisplayListItem::IFrame {
                child_dom_id: *child_dom_id,
                bounds: offset_rect_y(*bounds, y_offset),
                clip_rect: offset_rect_y(*clip_rect, y_offset),
            }
        }
        // Pass through stateless items
        DisplayListItem::PopClip => DisplayListItem::PopClip,
        DisplayListItem::PopScrollFrame => DisplayListItem::PopScrollFrame,
        DisplayListItem::PopStackingContext => DisplayListItem::PopStackingContext,
    }
}

/// Generate display list items for simple text (headers/footers).
/// 
/// This creates a simplified text rendering without full text layout.
/// For now, this creates a placeholder that renderers should handle specially.
fn generate_text_display_items(
    text: &str,
    bounds: LogicalRect,
    font_size: f32,
    color: ColorU,
    alignment: TextAlignment,
) -> Vec<DisplayListItem> {
    use crate::font_traits::FontHash;
    
    if text.is_empty() {
        return Vec::new();
    }
    
    // Calculate approximate text position based on alignment
    // For now, we estimate character width as 0.5 * font_size (monospace approximation)
    let char_width = font_size * 0.5;
    let text_width = text.len() as f32 * char_width;
    
    let x_offset = match alignment {
        TextAlignment::Left => bounds.origin.x,
        TextAlignment::Center => bounds.origin.x + (bounds.size.width - text_width) / 2.0,
        TextAlignment::Right => bounds.origin.x + bounds.size.width - text_width,
    };
    
    // Position text vertically centered in the bounds
    let y_pos = bounds.origin.y + (bounds.size.height + font_size) / 2.0 - font_size * 0.2;
    
    // Create simple glyph instances for each character
    // Note: This is a simplified approach - proper text rendering should use text3
    let glyphs: Vec<GlyphInstance> = text
        .chars()
        .enumerate()
        .filter(|(_, c)| !c.is_control())
        .map(|(i, c)| GlyphInstance {
            index: c as u32,  // Use Unicode codepoint as glyph index (placeholder)
            point: LogicalPosition {
                x: x_offset + i as f32 * char_width,
                y: y_pos,
            },
            size: LogicalSize::new(char_width, font_size),
        })
        .collect();
    
    if glyphs.is_empty() {
        return Vec::new();
    }
    
    vec![DisplayListItem::Text {
        glyphs,
        font_hash: FontHash::from_hash(0), // Default font hash - renderer should use default font
        font_size_px: font_size,
        color,
        clip_rect: bounds,
    }]
}

/// Calculate the total height of a display list (max Y + height of all items).
fn calculate_display_list_height(display_list: &DisplayList) -> f32 {
    let mut max_bottom = 0.0f32;
    
    for item in &display_list.items {
        if let Some(bounds) = get_display_item_bounds(item) {
            let item_bottom = bounds.origin.y + bounds.size.height;
            max_bottom = max_bottom.max(item_bottom);
        }
    }
    
    max_bottom
}

/// Advanced pagination that combines slicer approach with CSS break properties.
/// 
/// This function:
/// 1. First analyzes break properties (break-before, break-after, break-inside)
/// 2. Identifies "monolithic" items that should NOT be split
/// 3. Uses slicer clipping for splittable content
/// 4. Pushes monolithic items to next page if they don't fit
/// 
/// # Arguments
/// * `display_list` - The complete display list from layout
/// * `node_mapping` - Maps display list items to DOM nodes (for break property lookup)
/// * `config` - Slicer configuration
/// * `styled_dom` - The styled DOM (for reading CSS break properties)
pub fn paginate_with_break_properties<F>(
    full_display_list: DisplayList,
    config: &SlicerConfig,
    is_monolithic: F,
) -> Result<Vec<DisplayList>> 
where
    F: Fn(Option<azul_core::dom::NodeId>) -> bool,
{
    if config.page_content_height <= 0.0 {
        return Ok(vec![full_display_list]);
    }
    
    // Collect items with metadata
    let items_with_meta: Vec<(DisplayListItem, Option<LogicalRect>, Option<azul_core::dom::NodeId>, bool)> = 
        full_display_list.items
            .into_iter()
            .zip(full_display_list.node_mapping.iter())
            .map(|(item, node_id)| {
                let bounds = get_display_item_bounds(&item);
                let node_id = *node_id;
                let monolithic = is_monolithic(node_id);
                (item, bounds, node_id, monolithic)
            })
            .collect();
    
    // Track page assignments and shifts
    let mut pages: Vec<Vec<DisplayListItem>> = vec![Vec::new()];
    let mut page_node_mappings: Vec<Vec<Option<azul_core::dom::NodeId>>> = vec![Vec::new()];
    let mut shift_amount = 0.0f32;
    
    for (item, bounds, node_id, is_monolithic_item) in items_with_meta {
        let Some(item_bounds) = bounds else {
            // Items without bounds - skip for now
            continue;
        };
        
        let shifted_y = item_bounds.origin.y + shift_amount;
        let item_height = item_bounds.size.height;
        
        let page_idx = config.page_for_y(shifted_y);
        let (page_top, page_bottom) = config.page_bounds(page_idx);
        let remaining_on_page = page_bottom - shifted_y;
        
        if is_monolithic_item {
            // Monolithic item: cannot be split
            if item_height <= remaining_on_page {
                // Fits on current page
                let clipped = clip_and_offset_display_item(&item, page_top, page_bottom);
                if let Some(clipped_item) = clipped {
                    ensure_page_exists(&mut pages, &mut page_node_mappings, page_idx);
                    pages[page_idx].push(clipped_item);
                    page_node_mappings[page_idx].push(node_id);
                }
            } else if item_height <= config.page_content_height {
                // Doesn't fit but would fit on empty page - push to next page
                let next_page = page_idx + 1;
                let (next_top, next_bottom) = config.page_bounds(next_page);
                
                // Update shift for this and subsequent items
                let new_position = next_top;
                let extra_shift = new_position - item_bounds.origin.y;
                if extra_shift > shift_amount {
                    shift_amount = extra_shift;
                }
                
                let clipped = clip_and_offset_display_item(&item, next_top - extra_shift + shift_amount, next_bottom - extra_shift + shift_amount);
                if let Some(clipped_item) = clipped {
                    ensure_page_exists(&mut pages, &mut page_node_mappings, next_page);
                    pages[next_page].push(clipped_item);
                    page_node_mappings[next_page].push(node_id);
                }
            } else {
                // Too large for any page - let it overflow (place on current page)
                let clipped = clip_and_offset_display_item(&item, page_top, page_bottom);
                if let Some(clipped_item) = clipped {
                    ensure_page_exists(&mut pages, &mut page_node_mappings, page_idx);
                    pages[page_idx].push(clipped_item);
                    page_node_mappings[page_idx].push(node_id);
                }
            }
        } else {
            // Splittable item: can be clipped across pages
            let shifted_bounds = LogicalRect {
                origin: LogicalPosition { x: item_bounds.origin.x, y: shifted_y },
                size: item_bounds.size,
            };
            let item_bottom = shifted_y + item_height;
            
            // Find all pages this item overlaps
            let start_page = config.page_for_y(shifted_y);
            let end_page = config.page_for_y(item_bottom - 0.01);
            
            for p in start_page..=end_page {
                let (p_top, p_bottom) = config.page_bounds(p);
                // Adjust item for the shift when clipping
                let y_delta = shifted_y - item_bounds.origin.y;
                let adjusted_item = offset_display_item_y(&item, y_delta);
                if let Some(clipped) = clip_and_offset_display_item(&adjusted_item, p_top, p_bottom) {
                    ensure_page_exists(&mut pages, &mut page_node_mappings, p);
                    pages[p].push(clipped);
                    page_node_mappings[p].push(node_id);
                }
            }
        }
    }
    
    // Convert to DisplayList format
    let result: Vec<DisplayList> = pages
        .into_iter()
        .zip(page_node_mappings.into_iter())
        .map(|(items, mappings)| DisplayList {
            items,
            node_mapping: mappings,
        })
        .collect();
    
    if result.is_empty() {
        return Ok(vec![DisplayList::default()]);
    }
    
    Ok(result)
}

/// Helper to ensure pages vector has enough entries.
fn ensure_page_exists(
    pages: &mut Vec<Vec<DisplayListItem>>,
    mappings: &mut Vec<Vec<Option<azul_core::dom::NodeId>>>,
    page_idx: usize,
) {
    while pages.len() <= page_idx {
        pages.push(Vec::new());
    }
    while mappings.len() <= page_idx {
        mappings.push(Vec::new());
    }
}

// =============================================================================
// COMMITMENT-BASED PAGINATION WITH CSS BREAK PROPERTIES
// =============================================================================
//
// This approach combines the slicer model with CSS fragmentation properties:
// 1. Pre-scan to identify "Keep-Together Groups" (break-after: avoid)
// 2. Single-pass placement with commitment (no backtracking)
// 3. Decision leeway to avoid tiny fragments
// 4. Force breaks for break-before: always

/// Break property information for pagination decisions.
#[derive(Debug, Clone, Copy, Default)]
pub struct BreakProperties {
    pub break_before: PageBreak,
    pub break_after: PageBreak,
    pub break_inside: BreakInside,
}

/// Paginate a display list using a "commitment" model with CSS break property support.
/// 
/// This function:
/// 1. Pre-processes to identify "Keep-Together Groups" (e.g., Heading + Paragraph)
/// 2. Uses a single-pass greedy placement algorithm
/// 3. Respects `break-before: always` for forced page breaks
/// 4. Respects `break-after: avoid` by grouping with subsequent content
/// 5. Uses "decision leeway" to avoid tiny fragments at page boundaries
/// 
/// # Arguments
/// * `full_display_list` - The complete display list from layout
/// * `config` - Slicer configuration (page height, headers/footers, etc.)
/// * `get_break_properties` - Closure to lookup CSS break properties for a node
pub fn paginate_display_list_with_commitment<F>(
    full_display_list: DisplayList,
    config: &SlicerConfig,
    get_break_properties: F,
) -> Result<Vec<DisplayList>> 
where
    F: Fn(Option<azul_core::dom::NodeId>) -> BreakProperties,
{
    use std::collections::HashMap;
    
    if config.page_content_height <= 0.0 || config.page_content_height >= f32::MAX {
        return Ok(vec![full_display_list]);
    }
    
    let page_content_height = config.page_content_height;
    
    // Phase 1: Identify "Keep-Together Groups" (break-after: avoid chains)
    // Items with break-after: avoid are grouped with their immediate successor
    let num_items = full_display_list.items.len();
    let mut item_group_ids = vec![0usize; num_items];
    let mut current_group_id = 1usize;
    
    for i in 0..num_items {
        let node_id = full_display_list.node_mapping.get(i).copied().flatten();
        let props = get_break_properties(node_id);
        
        // If this item says "avoid break after", group it with the next item
        if is_avoid_break(props.break_after) && i + 1 < num_items {
            if item_group_ids[i] == 0 {
                item_group_ids[i] = current_group_id;
            }
            // Add next item to same group
            item_group_ids[i + 1] = item_group_ids[i];
            
            // Check if the chain continues
            let next_node_id = full_display_list.node_mapping.get(i + 1).copied().flatten();
            let next_props = get_break_properties(next_node_id);
            if !is_avoid_break(next_props.break_after) {
                current_group_id += 1;
            }
        }
    }
    
    // Phase 2: Collect items with metadata, sorted by Y position
    let mut items_with_meta: Vec<_> = full_display_list.items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let bounds = get_display_item_bounds(item)?;
            let node_id = full_display_list.node_mapping.get(i).copied().flatten();
            let props = get_break_properties(node_id);
            let group_id = item_group_ids[i];
            Some((i, item.clone(), bounds, node_id, props, group_id))
        })
        .collect();
    
    // Sort by Y position to process in layout order
    items_with_meta.sort_by(|a, b| {
        a.2.origin.y.partial_cmp(&b.2.origin.y).unwrap_or(std::cmp::Ordering::Equal)
    });
    
    // Phase 3: Calculate group bounds (for keeping groups together)
    let mut group_bounds: HashMap<usize, (f32, f32)> = HashMap::new(); // group_id -> (min_y, max_y)
    for &(_, _, ref bounds, _, _, group_id) in &items_with_meta {
        if group_id > 0 {
            let item_top = bounds.origin.y;
            let item_bottom = bounds.origin.y + bounds.size.height;
            group_bounds
                .entry(group_id)
                .and_modify(|(min_y, max_y)| {
                    *min_y = min_y.min(item_top);
                    *max_y = max_y.max(item_bottom);
                })
                .or_insert((item_top, item_bottom));
        }
    }
    
    // Phase 4: Single-pass placement with commitment
    let mut pages: Vec<Vec<DisplayListItem>> = vec![Vec::new()];
    let mut page_node_mappings: Vec<Vec<Option<azul_core::dom::NodeId>>> = vec![Vec::new()];
    
    // Track cumulative shift - ONLY used for forced breaks (break-before/after: always)
    // This is NOT applied to normal items - only to items that come after a forced break
    let mut cumulative_shift = 0.0f32;
    
    // Track if the NEXT item needs to be shifted (due to break-after on previous item)
    let mut pending_break_after_shift: Option<f32> = None;
    
    // Track which groups have been committed to which page
    let mut group_page_commitment: HashMap<usize, usize> = HashMap::new();
    
    // Track items we've already placed (by original index) to avoid duplicates
    let mut placed_items: std::collections::HashSet<usize> = std::collections::HashSet::new();
    
    for (orig_idx, item, bounds, node_id, props, group_id) in items_with_meta {
        if placed_items.contains(&orig_idx) {
            continue;
        }
        
        // Apply any pending shift from a previous break-after
        if let Some(pending_shift) = pending_break_after_shift.take() {
            cumulative_shift = pending_shift;
        }
        
        // For normal items (no break-before), use original position
        // Only apply cumulative_shift if there was a preceding forced break
        let item_shift = if is_forced_break(props.break_before) || cumulative_shift > 0.0 {
            cumulative_shift
        } else {
            0.0
        };
        
        let shifted_y = bounds.origin.y + item_shift;
        let item_height = bounds.size.height;
        
        // Determine natural page for this item
        let natural_page = (shifted_y / page_content_height).floor() as usize;
        let mut target_page = natural_page;
        let mut this_item_shift = item_shift;
        
        // Check for forced break-before
        if is_forced_break(props.break_before) {
            let page_start_y = target_page as f32 * page_content_height;
            // Only force break if we're not already at the top of a page
            if shifted_y > page_start_y + 1.0 {
                target_page += 1;
                // Calculate shift to push this item to the start of the next page
                let new_page_start = target_page as f32 * page_content_height;
                this_item_shift = new_page_start - bounds.origin.y;
                cumulative_shift = this_item_shift;
            }
        }
        
        // Check group commitment (if part of a group, follow the group's page)
        if group_id > 0 {
            if let Some(&committed_page) = group_page_commitment.get(&group_id) {
                if committed_page > target_page {
                    target_page = committed_page;
                }
            }
        }
        
        // Recalculate position after potential shift updates
        let shifted_y = bounds.origin.y + this_item_shift;
        let page_end_y = (target_page + 1) as f32 * page_content_height;
        let remaining_space = page_end_y - shifted_y;
        
        // Decision leeway: If the item (or group) would leave a tiny fragment, push it
        let effective_height = if group_id > 0 {
            // Use group height for decision
            if let Some(&(group_min, group_max)) = group_bounds.get(&group_id) {
                group_max - group_min
            } else {
                item_height
            }
        } else {
            item_height
        };
        
        let fits_on_page = effective_height <= remaining_space;
        let fits_on_empty_page = effective_height <= page_content_height;
        
        // Check if we should push to next page (ONLY for break-inside: avoid scenarios)
        // For normal content, we let it flow naturally across pages
        let should_push = if !fits_on_page && fits_on_empty_page {
            // Only push if break_inside is avoid AND the item can fit on an empty page
            props.break_inside == BreakInside::Avoid
        } else {
            false
        };
        
        if should_push {
            target_page += 1;
            let new_page_start = target_page as f32 * page_content_height;
            this_item_shift = new_page_start - bounds.origin.y;
            // Don't propagate this shift to subsequent items unless there's a break-after
        }
        
        // Commit group to this page
        if group_id > 0 {
            group_page_commitment.insert(group_id, target_page);
        }
        
        // Place the item (potentially split across pages)
        let final_y = bounds.origin.y + this_item_shift;
        let final_bottom = final_y + item_height;
        
        let start_page = (final_y / page_content_height).floor() as usize;
        let end_page = ((final_bottom - 0.01).max(final_y) / page_content_height).floor() as usize;
        
        for p in start_page..=end_page {
            let p_top = p as f32 * page_content_height;
            let p_bottom = (p + 1) as f32 * page_content_height;
            
            // Create a shifted version of the item (only if shift is needed)
            let item_to_clip = if this_item_shift != 0.0 {
                offset_display_item_y(&item, this_item_shift)
            } else {
                item.clone()
            };
            
            if let Some(clipped) = clip_and_offset_display_item(&item_to_clip, p_top, p_bottom) {
                ensure_page_exists(&mut pages, &mut page_node_mappings, p);
                pages[p].push(clipped);
                page_node_mappings[p].push(node_id);
            }
        }
        
        placed_items.insert(orig_idx);
        
        // Handle break-after: always
        // After placing this item, set up a pending shift for the NEXT item
        if is_forced_break(props.break_after) {
            let item_end_page = end_page;
            let next_page_start = (item_end_page + 1) as f32 * page_content_height;
            // Calculate how much the next item needs to be shifted to land at next page start
            // The next item's original Y will need this much shift
            pending_break_after_shift = Some(next_page_start - bounds.origin.y - item_height);
        }
    }
    
    // Phase 5: Handle items without bounds (stateless items like PopClip)
    // For now, skip them as they don't affect visual output
    
    // Phase 6: Add headers and footers if configured
    let num_pages = pages.len();
    for page_idx in 0..num_pages {
        let page_info = PageInfo::new(page_idx + 1, num_pages);
        let skip_this_page = config.header_footer.skip_first_page && page_info.is_first;
        
        // Add header
        if config.header_footer.show_header && !skip_this_page {
            let header_text = config.header_footer.header_text(page_info);
            if !header_text.is_empty() {
                let header_items = generate_text_display_items(
                    &header_text,
                    LogicalRect {
                        origin: LogicalPosition { x: 0.0, y: 0.0 },
                        size: LogicalSize { 
                            width: config.page_width, 
                            height: config.header_footer.header_height 
                        },
                    },
                    config.header_footer.font_size,
                    config.header_footer.text_color,
                    TextAlignment::Center,
                );
                // Insert headers at the beginning
                for (i, item) in header_items.into_iter().enumerate() {
                    pages[page_idx].insert(i, item);
                    page_node_mappings[page_idx].insert(i, None);
                }
            }
        }
        
        // Add footer
        if config.header_footer.show_footer && !skip_this_page {
            let footer_text = config.header_footer.footer_text(page_info);
            if !footer_text.is_empty() {
                let footer_y = page_content_height - config.header_footer.footer_height;
                let footer_items = generate_text_display_items(
                    &footer_text,
                    LogicalRect {
                        origin: LogicalPosition { x: 0.0, y: footer_y },
                        size: LogicalSize { 
                            width: config.page_width, 
                            height: config.header_footer.footer_height 
                        },
                    },
                    config.header_footer.font_size,
                    config.header_footer.text_color,
                    TextAlignment::Center,
                );
                for item in footer_items {
                    pages[page_idx].push(item);
                    page_node_mappings[page_idx].push(None);
                }
            }
        }
    }
    
    // Convert to DisplayList format
    let result: Vec<DisplayList> = pages
        .into_iter()
        .zip(page_node_mappings.into_iter())
        .map(|(items, mappings)| DisplayList { items, node_mapping: mappings })
        .collect();
    
    Ok(if result.is_empty() { vec![DisplayList::default()] } else { result })
}
