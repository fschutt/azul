//! solver3/mod.rs
//!
//! Next-generation CSS layout engine with proper formatting context separation

pub mod cache;
pub mod display_list;
pub mod fc;
pub mod geometry;
pub mod layout_tree;
pub mod positioning;
pub mod sizing;

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    app_resources::RendererResources,
    callbacks::{DocumentId, ScrollPosition},
    dom::NodeId,
    styled_dom::{DomId, StyledDom},
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::{CssProperty, CssPropertyCategory, LayoutDebugMessage};

use self::{
    display_list::generate_display_list,
    geometry::IntrinsicSizes,
    layout_tree::{generate_layout_tree, LayoutTree},
    sizing::calculate_intrinsic_sizes,
};
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
    cache: &mut LayoutCache,
    new_dom: StyledDom,
    viewport: LogicalRect,
    font_manager: &FontManager<T, Q>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<DisplayList> {
    let mut ctx = LayoutContext {
        styled_dom: &new_dom,
        font_manager,
        debug_messages,
    };

    // --- Step 1: Reconciliation & Invalidation ---
    let (mut new_tree, mut recon_result) =
        cache::reconcile_and_invalidate(&mut ctx, cache, viewport)?;

    // --- Step 1.5: Early Exit Optimization ---
    if recon_result.is_clean() {
        ctx.debug_log("No changes, returning existing display list");
        let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;
        return generate_display_list(&mut ctx, tree, &cache.absolute_positions, scroll_offsets);
    }

    // --- Step 2: Incremental Layout Loop (handles scrollbar-induced reflows) ---
    let mut absolute_positions;
    loop {
        absolute_positions = cache.absolute_positions.clone();
        let mut reflow_needed_for_scrollbars = false;

        // Pass 2a (Incremental): Recalculate intrinsic sizes for dirty nodes (bottom-up).
        calculate_intrinsic_sizes(&mut ctx, &mut new_tree, &recon_result.intrinsic_dirty)?;

        // Pass 2b (Incremental): Recalculate layout for dirty subtrees (top-down).
        // This single pass now handles both sizing and in-flow positioning.
        for &root_idx in &recon_result.layout_roots {
            let (cb_pos, cb_size) =
                get_containing_block_for_node(&new_tree, root_idx, &absolute_positions, viewport);

            cache::calculate_layout_for_subtree(
                &mut ctx,
                &mut new_tree,
                root_idx,
                cb_pos,
                cb_size,
                &mut absolute_positions,
                &mut reflow_needed_for_scrollbars,
            )?;
        }

        // Pass 2c: Reposition clean sibling subtrees.
        cache::reposition_clean_subtrees(
            &new_tree,
            &recon_result.layout_roots,
            &mut absolute_positions,
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
        &mut absolute_positions,
        viewport,
    )?;

    // --- Step 3.5: Adjust Relatively Positioned Elements ---
    positioning::adjust_relative_positions(&mut ctx, &new_tree, &mut absolute_positions)?;

    // --- Step 4: Generate Display List & Update Cache ---
    let display_list =
        generate_display_list(&mut ctx, &new_tree, &absolute_positions, scroll_offsets)?;

    cache.tree = Some(new_tree);
    cache.absolute_positions = absolute_positions;
    cache.viewport = Some(viewport);

    Ok(display_list)
}

// STUB: This helper is required by the main loop
fn get_containing_block_for_node<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    node_idx: usize,
    absolute_positions: &BTreeMap<usize, LogicalPosition>,
    viewport: LogicalRect,
) -> (LogicalPosition, LogicalSize) {
    if let Some(parent_idx) = tree.get(node_idx).and_then(|n| n.parent) {
        if let Some(parent_node) = tree.get(parent_idx) {
            let pos = absolute_positions
                .get(&parent_idx)
                .copied()
                .unwrap_or_default();
            let size = parent_node.used_size.unwrap_or_default();
            let content_pos = LogicalPosition::new(
                pos.x + parent_node.box_props.padding.left,
                pos.y + parent_node.box_props.padding.top,
            );
            let writing_mode = get_writing_mode(parent_node.dom_node_id);
            let content_size = parent_node.box_props.inner_size(size, writing_mode);
            return (content_pos, content_size);
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
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::InvalidTree => write!(f, "Invalid layout tree"),
            LayoutError::SizingFailed => write!(f, "Sizing calculation failed"),
            LayoutError::PositioningFailed => write!(f, "Position calculation failed"),
            LayoutError::DisplayListFailed => write!(f, "Display list generation failed"),
        }
    }
}

impl std::error::Error for LayoutError {}

pub type Result<T> = std::result::Result<T, LayoutError>;
