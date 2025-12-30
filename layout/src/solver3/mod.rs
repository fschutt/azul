//! solver3/mod.rs
//!
//! Next-generation CSS layout engine with proper formatting context separation

pub mod cache;
pub mod counters;
pub mod display_list;
pub mod fc;
pub mod geometry;
pub mod getters;
pub mod layout_tree;
pub mod paged_layout;
pub mod pagination;
pub mod positioning;
pub mod scrollbar;
pub mod sizing;
pub mod taffy_bridge;

/// Lazy debug_info macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_info {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_info_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_warning macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_warning {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_warning_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_error macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_error {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_error_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_log macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_log {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_log_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_box_props macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_box_props {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_box_props_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_css_getter macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_css_getter {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_css_getter_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_bfc_layout macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_bfc_layout {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_bfc_layout_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_ifc_layout macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_ifc_layout {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_ifc_layout_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_table_layout macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_table_layout {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_table_layout_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_display_type macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_display_type {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_display_type_inner(format!($($arg)*));
        }
    };
}

// Test modules commented out until they are implemented
// #[cfg(test)]
// mod tests;
// #[cfg(test)]
// mod tests_arabic;

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::{DocumentId, ScrollPosition},
    resources::RendererResources,
    selection::SelectionState,
    styled_dom::StyledDom,
};
use azul_css::{
    props::property::{CssProperty, CssPropertyCategory},
    LayoutDebugMessage, LayoutDebugMessageType,
};

use self::{
    display_list::generate_display_list,
    geometry::IntrinsicSizes,
    getters::get_writing_mode,
    layout_tree::{generate_layout_tree, LayoutTree},
    sizing::calculate_intrinsic_sizes,
};
#[cfg(feature = "text_layout")]
pub use crate::font_traits::TextLayoutCache;
use crate::{
    font_traits::ParsedFontTrait,
    solver3::{
        cache::LayoutCache,
        display_list::DisplayList,
        fc::{check_scrollbar_necessity, LayoutConstraints, LayoutResult},
        layout_tree::DirtyFlag,
    },
};

/// A map of hashes for each node to detect changes in content like text.
pub type NodeHashMap = BTreeMap<usize, u64>;

/// Central context for a single layout pass.
pub struct LayoutContext<'a, T: ParsedFontTrait> {
    pub styled_dom: &'a StyledDom,
    #[cfg(feature = "text_layout")]
    pub font_manager: &'a crate::font_traits::FontManager<T>,
    #[cfg(not(feature = "text_layout"))]
    pub font_manager: core::marker::PhantomData<&'a T>,
    pub selections: &'a BTreeMap<DomId, SelectionState>,
    pub debug_messages: &'a mut Option<Vec<LayoutDebugMessage>>,
    pub counters: &'a mut BTreeMap<(usize, String), i32>,
    pub viewport_size: LogicalSize,
    /// Fragmentation context for CSS Paged Media (PDF generation)
    /// When Some, layout respects page boundaries and generates one DisplayList per page
    pub fragmentation_context: Option<&'a mut crate::paged::FragmentationContext>,
}

impl<'a, T: ParsedFontTrait> LayoutContext<'a, T> {
    /// Check if debug messages are enabled (for use with lazy macros)
    #[inline]
    pub fn has_debug(&self) -> bool {
        self.debug_messages.is_some()
    }

    /// Internal method - called by debug_log! macro after checking has_debug()
    #[inline]
    pub fn debug_log_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage {
                message: message.into(),
                location: "solver3".into(),
                message_type: Default::default(),
            });
        }
    }

    /// Internal method - called by debug_info! macro after checking has_debug()
    #[inline]
    pub fn debug_info_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::info(message));
        }
    }

    /// Internal method - called by debug_warning! macro after checking has_debug()
    #[inline]
    pub fn debug_warning_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::warning(message));
        }
    }

    /// Internal method - called by debug_error! macro after checking has_debug()
    #[inline]
    pub fn debug_error_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::error(message));
        }
    }

    /// Internal method - called by debug_box_props! macro after checking has_debug()
    #[inline]
    pub fn debug_box_props_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::box_props(message));
        }
    }

    /// Internal method - called by debug_css_getter! macro after checking has_debug()
    #[inline]
    pub fn debug_css_getter_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::css_getter(message));
        }
    }

    /// Internal method - called by debug_bfc_layout! macro after checking has_debug()
    #[inline]
    pub fn debug_bfc_layout_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::bfc_layout(message));
        }
    }

    /// Internal method - called by debug_ifc_layout! macro after checking has_debug()
    #[inline]
    pub fn debug_ifc_layout_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::ifc_layout(message));
        }
    }

    /// Internal method - called by debug_table_layout! macro after checking has_debug()
    #[inline]
    pub fn debug_table_layout_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::table_layout(message));
        }
    }

    /// Internal method - called by debug_display_type! macro after checking has_debug()
    #[inline]
    pub fn debug_display_type_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::display_type(message));
        }
    }

    // DEPRECATED: Use debug_*!() macros instead for lazy evaluation
    // These methods always evaluate format!() arguments even when debug is disabled

    #[inline]
    #[deprecated(note = "Use debug_info! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_info(&mut self, message: impl Into<String>) {
        self.debug_info_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_warning! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_warning(&mut self, message: impl Into<String>) {
        self.debug_warning_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_error! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_error(&mut self, message: impl Into<String>) {
        self.debug_error_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_log! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_log(&mut self, message: &str) {
        self.debug_log_inner(message.to_string());
    }

    #[inline]
    #[deprecated(note = "Use debug_box_props! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_box_props(&mut self, message: impl Into<String>) {
        self.debug_box_props_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_css_getter! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_css_getter(&mut self, message: impl Into<String>) {
        self.debug_css_getter_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_bfc_layout! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_bfc_layout(&mut self, message: impl Into<String>) {
        self.debug_bfc_layout_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_ifc_layout! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_ifc_layout(&mut self, message: impl Into<String>) {
        self.debug_ifc_layout_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_table_layout! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_table_layout(&mut self, message: impl Into<String>) {
        self.debug_table_layout_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_display_type! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_display_type(&mut self, message: impl Into<String>) {
        self.debug_display_type_inner(message.into());
    }
}

/// Main entry point for the incremental, cached layout engine
#[cfg(feature = "text_layout")]
pub fn layout_document<T: ParsedFontTrait + Sync + 'static>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    new_dom: StyledDom,
    viewport: LogicalRect,
    font_manager: &crate::font_traits::FontManager<T>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    selections: &BTreeMap<DomId, SelectionState>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &azul_core::resources::RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: azul_core::dom::DomId,
) -> Result<DisplayList> {
    if let Some(msgs) = debug_messages.as_mut() {
        msgs.push(LayoutDebugMessage::info(format!("[Layout] layout_document called - viewport: ({:.1}, {:.1}) size ({:.1}x{:.1})",
            viewport.origin.x, viewport.origin.y, viewport.size.width, viewport.size.height)));
        msgs.push(LayoutDebugMessage::info(format!("[Layout] DOM has {} nodes", new_dom.node_data.len())));
    }
    
    // Create temporary context without counters for tree generation
    let mut counter_values = BTreeMap::new();
    let mut ctx_temp = LayoutContext {
        styled_dom: &new_dom,
        font_manager,
        selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: None,
    };

    // --- Step 1: Reconciliation & Invalidation ---
    let (mut new_tree, mut recon_result) =
        cache::reconcile_and_invalidate(&mut ctx_temp, cache, viewport)?;

    // Step 1.2: Clear Taffy Caches for Dirty Nodes
    for &node_idx in &recon_result.intrinsic_dirty {
        if let Some(node) = new_tree.get_mut(node_idx) {
            node.taffy_cache.clear();
        }
    }

    // Step 1.3: Compute CSS Counters
    // This must be done after tree generation but before layout,
    // as list markers need counter values during formatting context layout
    cache::compute_counters(&new_dom, &new_tree, &mut counter_values);

    // Now create the real context with computed counters
    let mut ctx = LayoutContext {
        styled_dom: &new_dom,
        font_manager,
        selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: None,
    };

    // --- Step 1.5: Early Exit Optimization ---
    if recon_result.is_clean() {
        ctx.debug_log("No changes, returning existing display list");
        let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;

        // Use cached scroll IDs if available, otherwise compute them
        let scroll_ids = if cache.scroll_ids.is_empty() {
            use crate::window::LayoutWindow;
            let (scroll_ids, scroll_id_to_node_id) =
                LayoutWindow::compute_scroll_ids(tree, &new_dom);
            cache.scroll_ids = scroll_ids.clone();
            cache.scroll_id_to_node_id = scroll_id_to_node_id;
            scroll_ids
        } else {
            cache.scroll_ids.clone()
        };

        return generate_display_list(
            &mut ctx,
            tree,
            &cache.calculated_positions,
            scroll_offsets,
            &scroll_ids,
            gpu_value_cache,
            renderer_resources,
            id_namespace,
            dom_id,
        );
    }

    // --- Step 2: Incremental Layout Loop (handles scrollbar-induced reflows) ---
    let mut calculated_positions = cache.calculated_positions.clone();
    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > 10 {
            // Safety limit to prevent infinite loops
            break;
        }

        calculated_positions = cache.calculated_positions.clone();
        let mut reflow_needed_for_scrollbars = false;

        calculate_intrinsic_sizes(&mut ctx, &mut new_tree, &recon_result.intrinsic_dirty)?;

        for &root_idx in &recon_result.layout_roots {
            let (cb_pos, cb_size) = get_containing_block_for_node(
                &new_tree,
                &new_dom,
                root_idx,
                &calculated_positions,
                viewport,
            );

            // DEBUG: Log containing block info for this root
            if let Some(debug_msgs) = ctx.debug_messages.as_mut() {
                let root_node = &new_tree.nodes[root_idx];
                let dom_name = root_node
                    .dom_node_id
                    .and_then(|id| new_dom.node_data.as_container().internal.get(id.index()))
                    .map(|n| format!("{:?}", n.node_type))
                    .unwrap_or_else(|| "Unknown".to_string());

                debug_msgs.push(LayoutDebugMessage::new(
                    LayoutDebugMessageType::PositionCalculation,
                    format!(
                        "[LAYOUT ROOT {}] {} - CB pos=({:.2}, {:.2}), CB size=({:.2}x{:.2}), \
                         viewport=({:.2}x{:.2})",
                        root_idx,
                        dom_name,
                        cb_pos.x,
                        cb_pos.y,
                        cb_size.width,
                        cb_size.height,
                        viewport.size.width,
                        viewport.size.height
                    ),
                ));
            }

            cache::calculate_layout_for_subtree(
                &mut ctx,
                &mut new_tree,
                text_cache,
                root_idx,
                cb_pos,
                cb_size,
                &mut calculated_positions,
                &mut reflow_needed_for_scrollbars,
                &mut cache.float_cache,
            )?;

            // CRITICAL: Insert the root node's own position into calculated_positions
            // This is necessary because calculate_layout_for_subtree only inserts
            // positions for children, not for the root itself.
            if !calculated_positions.contains_key(&root_idx) {
                let root_node = &new_tree.nodes[root_idx];

                // DEBUG: Log root positioning
                if let Some(debug_msgs) = ctx.debug_messages.as_mut() {
                    let dom_name = root_node
                        .dom_node_id
                        .and_then(|id| new_dom.node_data.as_container().internal.get(id.index()))
                        .map(|n| format!("{:?}", n.node_type))
                        .unwrap_or_else(|| "Unknown".to_string());

                    debug_msgs.push(LayoutDebugMessage::new(
                        LayoutDebugMessageType::PositionCalculation,
                        format!(
                            "[ROOT POSITION {}] {} - Inserting position=({:.2}, {:.2}), \
                             margin=({:.2}, {:.2}, {:.2}, {:.2})",
                            root_idx,
                            dom_name,
                            cb_pos.x,
                            cb_pos.y,
                            root_node.box_props.margin.top,
                            root_node.box_props.margin.right,
                            root_node.box_props.margin.bottom,
                            root_node.box_props.margin.left
                        ),
                    ));
                }

                calculated_positions.insert(root_idx, cb_pos);
            }
        }

        cache::reposition_clean_subtrees(
            &new_dom,
            &new_tree,
            &recon_result.layout_roots,
            &mut calculated_positions,
        );

        if reflow_needed_for_scrollbars {
            ctx.debug_log("Scrollbars changed container size, starting full reflow...");
            recon_result.layout_roots.clear();
            recon_result.layout_roots.insert(new_tree.root);
            recon_result.intrinsic_dirty = (0..new_tree.nodes.len()).collect();
            continue;
        }

        break;
    }

    // --- Step 3: Adjust Relatively Positioned Elements ---
    // This must be done BEFORE positioning out-of-flow elements, because
    // relatively positioned elements establish containing blocks for their
    // absolutely positioned descendants. If we adjust relative positions after
    // positioning absolute elements, the absolute elements will be positioned
    // relative to the wrong (pre-adjustment) position of their containing block.
    // Pass the viewport to correctly resolve percentage offsets for the root element.
    positioning::adjust_relative_positions(
        &mut ctx,
        &new_tree,
        &mut calculated_positions,
        viewport,
    )?;

    // --- Step 3.5: Position Out-of-Flow Elements ---
    // This must be done AFTER adjusting relative positions, so that absolutely
    // positioned elements are positioned relative to the final (post-adjustment)
    // position of their relatively positioned containing blocks.
    positioning::position_out_of_flow_elements(
        &mut ctx,
        &mut new_tree,
        &mut calculated_positions,
        viewport,
    )?;

    // --- Step 3.75: Compute Stable Scroll IDs ---
    // This must be done AFTER layout but BEFORE display list generation
    use crate::window::LayoutWindow;
    let (scroll_ids, scroll_id_to_node_id) = LayoutWindow::compute_scroll_ids(&new_tree, &new_dom);

    // --- Step 4: Generate Display List & Update Cache ---
    let display_list = generate_display_list(
        &mut ctx,
        &new_tree,
        &calculated_positions,
        scroll_offsets,
        &scroll_ids,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
    )?;

    cache.tree = Some(new_tree);
    cache.calculated_positions = calculated_positions;
    cache.viewport = Some(viewport);
    cache.scroll_ids = scroll_ids;
    cache.scroll_id_to_node_id = scroll_id_to_node_id;
    cache.counters = counter_values;

    Ok(display_list)
}

// STUB: This helper is required by the main loop
fn get_containing_block_for_node(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    node_idx: usize,
    calculated_positions: &BTreeMap<usize, LogicalPosition>,
    viewport: LogicalRect,
) -> (LogicalPosition, LogicalSize) {
    if let Some(parent_idx) = tree.get(node_idx).and_then(|n| n.parent) {
        if let Some(parent_node) = tree.get(parent_idx) {
            let pos = calculated_positions
                .get(&parent_idx)
                .copied()
                .unwrap_or_default();
            let size = parent_node.used_size.unwrap_or_default();
            // Position in calculated_positions is the margin-box position
            // To get content-box, add: border + padding (NOT margin, that's already in pos)
            let content_pos = LogicalPosition::new(
                pos.x + parent_node.box_props.border.left + parent_node.box_props.padding.left,
                pos.y + parent_node.box_props.border.top + parent_node.box_props.padding.top,
            );

            if let Some(dom_id) = parent_node.dom_node_id {
                let styled_node_state = &styled_dom
                    .styled_nodes
                    .as_container()
                    .get(dom_id)
                    .map(|n| &n.styled_node_state)
                    .cloned()
                    .unwrap_or_default();
                let writing_mode =
                    get_writing_mode(styled_dom, dom_id, styled_node_state).unwrap_or_default();
                let content_size = parent_node.box_props.inner_size(size, writing_mode);
                return (content_pos, content_size);
            }

            return (content_pos, size);
        }
    }
    (viewport.origin, viewport.size)
}

#[derive(Debug)]
pub enum LayoutError {
    InvalidTree,
    SizingFailed,
    PositioningFailed,
    DisplayListFailed,
    Text(crate::font_traits::LayoutError),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::InvalidTree => write!(f, "Invalid layout tree"),
            LayoutError::SizingFailed => write!(f, "Sizing calculation failed"),
            LayoutError::PositioningFailed => write!(f, "Position calculation failed"),
            LayoutError::DisplayListFailed => write!(f, "Display list generation failed"),
            LayoutError::Text(e) => write!(f, "Text layout error: {:?}", e),
        }
    }
}

impl From<crate::font_traits::LayoutError> for LayoutError {
    fn from(err: crate::font_traits::LayoutError) -> Self {
        LayoutError::Text(err)
    }
}

impl std::error::Error for LayoutError {}

pub type Result<T> = std::result::Result<T, LayoutError>;
