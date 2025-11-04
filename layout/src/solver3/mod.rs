//! solver3/mod.rs
//!
//! Next-generation CSS layout engine with proper formatting context separation

pub mod cache;
pub mod display_list;
pub mod fc;
pub mod geometry;
pub mod getters;
pub mod layout_tree;
pub mod positioning;
pub mod scrollbar;
pub mod sizing;
pub mod taffy_bridge;

#[cfg(test)]
mod tests;

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
    LayoutDebugMessage,
};

use self::{
    display_list::generate_display_list,
    geometry::IntrinsicSizes,
    getters::get_writing_mode,
    layout_tree::{generate_layout_tree, LayoutTree},
    sizing::calculate_intrinsic_sizes,
};
pub use crate::text3::cache::LayoutCache as TextLayoutCache;
use crate::{
    solver3::{
        cache::LayoutCache,
        display_list::DisplayList,
        fc::{check_scrollbar_necessity, LayoutConstraints, LayoutResult},
        layout_tree::DirtyFlag,
    },
    text3::{
        self,
        cache::{FontLoaderTrait, FontManager, ParsedFontTrait},
    },
};

/// A map of hashes for each node to detect changes in content like text.
pub type NodeHashMap = BTreeMap<usize, u64>;

/// Central context for a single layout pass.
pub struct LayoutContext<'a, T: ParsedFontTrait, Q: FontLoaderTrait<T>> {
    pub styled_dom: &'a StyledDom,
    pub font_manager: &'a FontManager<T, Q>,
    pub selections: &'a BTreeMap<DomId, SelectionState>,
    pub debug_messages: &'a mut Option<Vec<LayoutDebugMessage>>,
}

impl<'a, T: ParsedFontTrait, Q: FontLoaderTrait<T>> LayoutContext<'a, T, Q> {
    pub fn debug_log(&mut self, message: &str) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage {
                message: message.into(),
                location: "solver3".into(),
            });
        }
    }
}

/// Main entry point for the incremental, cached layout engine
pub fn layout_document<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    cache: &mut LayoutCache<T>,
    text_cache: &mut TextLayoutCache<T>,
    new_dom: StyledDom,
    viewport: LogicalRect,
    font_manager: &FontManager<T, Q>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    selections: &BTreeMap<DomId, SelectionState>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &azul_core::resources::RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: azul_core::dom::DomId,
) -> Result<DisplayList> {
    let mut ctx = LayoutContext {
        styled_dom: &new_dom,
        font_manager,
        selections,
        debug_messages,
    };

    // --- Step 1: Reconciliation & Invalidation ---
    let (mut new_tree, mut recon_result) =
        cache::reconcile_and_invalidate(&mut ctx, cache, viewport)?;

    // Step 1.2: Clear Taffy Caches for Dirty Nodes
    for &node_idx in &recon_result.intrinsic_dirty {
        if let Some(node) = new_tree.get_mut(node_idx) {
            node.taffy_cache.clear();
        }
    }

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
    let mut calculated_positions;
    loop {
        calculated_positions = cache.calculated_positions.clone();
        let mut reflow_needed_for_scrollbars = false;

        // ... (Passes 2a, 2b, 2c remain the same)
        calculate_intrinsic_sizes(&mut ctx, &mut new_tree, &recon_result.intrinsic_dirty)?;

        for &root_idx in &recon_result.layout_roots {
            let (cb_pos, cb_size) = get_containing_block_for_node(
                &new_tree,
                &new_dom,
                root_idx,
                &calculated_positions,
                viewport,
            );

            cache::calculate_layout_for_subtree(
                &mut ctx,
                &mut new_tree,
                text_cache,
                root_idx,
                cb_pos,
                cb_size,
                &mut calculated_positions,
                &mut reflow_needed_for_scrollbars,
            )?;

            // CRITICAL: Insert the root node's own position into calculated_positions
            // This is necessary because calculate_layout_for_subtree only inserts
            // positions for children, not for the root itself.
            if !calculated_positions.contains_key(&root_idx) {
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

    // --- Step 3: Position Out-of-Flow Elements ---
    positioning::position_out_of_flow_elements(
        &mut ctx,
        &new_tree,
        &mut calculated_positions,
        viewport,
    )?;

    // --- Step 3.5: Adjust Relatively Positioned Elements ---
    // Pass the viewport to correctly resolve percentage offsets for the root element.
    positioning::adjust_relative_positions(
        &mut ctx,
        &new_tree,
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

    Ok(display_list)
}

// STUB: This helper is required by the main loop
fn get_containing_block_for_node<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
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
            let content_pos = LogicalPosition::new(
                pos.x + parent_node.box_props.padding.left,
                pos.y + parent_node.box_props.padding.top,
            );

            if let Some(dom_id) = parent_node.dom_node_id {
                let styled_node_state = &styled_dom
                    .styled_nodes
                    .as_container()
                    .get(dom_id)
                    .map(|n| &n.state)
                    .cloned()
                    .unwrap_or_default();
                let writing_mode = get_writing_mode(styled_dom, dom_id, styled_node_state);
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
    Text(text3::cache::LayoutError),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::InvalidTree => write!(f, "Invalid layout tree"),
            LayoutError::SizingFailed => write!(f, "Sizing calculation failed"),
            LayoutError::PositioningFailed => write!(f, "Position calculation failed"),
            LayoutError::DisplayListFailed => write!(f, "Display list generation failed"),
            LayoutError::Text(e) => write!(f, "Text layout error: {}", e),
        }
    }
}

impl From<text3::cache::LayoutError> for LayoutError {
    fn from(err: text3::cache::LayoutError) -> Self {
        LayoutError::Text(err)
    }
}

impl std::error::Error for LayoutError {}

pub type Result<T> = std::result::Result<T, LayoutError>;
