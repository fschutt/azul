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
    ui_solver::IntrinsicSizes,
    window::{LogicalRect, LogicalSize},
};
use azul_css::{CssProperty, CssPropertyCategory, LayoutDebugMessage};

use self::{
    display_list::generate_display_list,
    layout_tree::{generate_layout_tree, LayoutTree},
    positioning::calculate_positions,
    sizing::{calculate_intrinsic_sizes, calculate_used_sizes},
};
use crate::{
    solver3::{
        cache::LayoutCache,
        fc::{check_scrollbar_necessity, LayoutConstraints, LayoutResult},
        layout_tree::DirtyFlag,
    },
    text3::cache::{FontLoaderTrait, FontManager, ParsedFontTrait},
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

/// Main entry point for the incremental, cached layout engine.
/// It is "pure functional" to the outside world, taking a DOM and returning a result,
/// while mutating an internal cache for performance.
pub fn layout_document<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    new_dom: StyledDom,
    cache: &mut LayoutCache,
    viewport: LogicalRect,
    font_manager: &FontManager<T, Q>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<LayoutResult> {
    let mut ctx = LayoutContext {
        styled_dom: &new_dom,
        font_manager,
        debug_messages,
    };

    // --- Step 1: Reconciliation & Invalidation ---
    // Compare the new StyledDom with the cached LayoutTree.
    // This produces a new tree and a set of dirty nodes to relayout.
    let (mut new_tree, mut recon_result) = reconcile_and_invalidate(&mut ctx, cache, viewport)?;

    // --- Step 1.5: Early Exit Optimization ---
    // If reconciliation found no changes, we can return the cached layout immediately.
    if recon_result.is_clean() {
        ctx.debug_log("No layout changes detected, returning cached result.");
        return Ok(LayoutResult {
            // NOTE: The display_list is None because it doesn't need to be resubmitted.
            // The caller should reuse the one from the previous frame.
            display_list: None,
            // Return other data from the cache.
            rects: cache.get_rectangles(),
            word_positions: cache.get_word_positions(),
            // ... other fields
        });
    }

    // --- Step 2: Incremental Layout Loop (handles scrollbar-induced reflows) ---
    let mut absolute_positions;
    loop {
        // Start with the previous frame's positions. Clean nodes will keep them
        // unless shifted by a dirty sibling.
        absolute_positions = cache.absolute_positions.clone();
        let mut reflow_needed_for_scrollbars = false;

        // Pass 2a (Incremental): Recalculate intrinsic sizes for dirty nodes (bottom-up).
        // This must be done for all dirty nodes before the top-down pass.
        if !recon_result.intrinsic_dirty.is_empty() {
            calculate_intrinsic_sizes_for_dirty_nodes(
                &mut ctx,
                &mut new_tree,
                &recon_result.intrinsic_dirty,
            )?;
        }

        // Pass 2b & 3 (Incremental): Recalculate layout for dirty subtrees (top-down).
        for &root_idx in &recon_result.layout_roots {
            // Determine the containing block for the layout root.
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

        // Pass 3.5: Reposition clean sibling subtrees.
        cache::reposition_clean_subtrees(
            &new_tree,
            &recon_result.layout_roots,
            &mut absolute_positions,
        );

        // --- Scrollbar Check ---
        if reflow_needed_for_scrollbars {
            ctx.debug_log("Scrollbars changed container size, starting full reflow...");
            // A scrollbar reflow invalidates everything below it. The simplest, most robust
            // way to handle this is to mark the entire tree as dirty and rerun the loop.
            recon_result.layout_roots.clear();
            recon_result.layout_roots.insert(new_tree.root);
            // Invalidate all intrinsic sizes as well, as they might depend on available space.
            recon_result.intrinsic_dirty = (0..new_tree.nodes.len()).collect();
            // Continue to the next iteration of the loop.
            continue;
        }

        // If we reach here, the layout is stable.
        break;
    }

    // --- Step 3.6: Position Out-of-Flow Elements ---
    // After the main flow layout is complete and stable, we run a separate pass
    // to correctly place `absolute` and `fixed` elements. This pass reads the
    // static positions calculated during the FC layout.
    position_out_of_flow_elements(&mut ctx, &new_tree, &mut absolute_positions, viewport)?;

    // --- Step 4: Generate Display List & Update Cache ---
    let display_list = generate_display_list(&mut ctx, &absolute_positions, scroll_offsets)?;

    // Update the cache for the next frame.
    cache.tree = Some(new_tree);
    cache.absolute_positions = absolute_positions;
    cache.viewport = Some(viewport); // Store the viewport size for the next frame's comparison

    // Construct the final result for the caller.
    Ok(LayoutResult {
        display_list: Some(display_list),
        rects: cache.get_rectangles(),
        word_positions: cache.get_word_positions(),
        // ... other fields
    })
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

/// Recursive, bottom-up pass to calculate intrinsic sizes ONLY for dirty nodes.
fn calculate_intrinsic_sizes_recursively<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree,
    node_index: usize,
) -> Result<()> {
    let node = tree
        .get(node_index)
        .cloned()
        .ok_or(LayoutError::InvalidTree)?;

    // Recurse to children first (post-order traversal)
    for &child_index in &node.children {
        calculate_intrinsic_sizes_recursively(ctx, tree, child_index)?;
    }

    let default_intrin = IntrinsicSizes::zero();

    // Now, if this node needs a layout update, calculate its intrinsic size
    if node.dirty_flag >= DirtyFlag::Layout {
        // This is a simplified version of the logic in sizing.rs
        // It would collect the now up-to-date intrinsic sizes from its children
        // and calculate its own.
        let children_intrinsics = node
            .children
            .iter()
            .map(|&c_idx| {
                (
                    c_idx,
                    tree.get(c_idx)
                        .and_then(|s| s.intrinsic_sizes.as_ref())
                        .unwrap_or(&default_intrin),
                )
            })
            .collect::<Vec<_>>();

        // let new_intrinsics = calculator.calculate_node_intrinsic_sizes(tree, node_index,
        // &children_intrinsics)?;
        let new_intrinsics = Default::default(); // STUB

        if let Some(n) = tree.get_mut(node_index) {
            n.intrinsic_sizes = Some(new_intrinsics);
        }
    }
    Ok(())
}

/// Recursive, top-down pass to calculate used sizes and positions ONLY for dirty subtrees.
fn calculate_layout_recursively<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree,
    node_index: usize,
    containing_block_size: LogicalSize,
    reflow_needed_for_scrollbars: &mut bool,
) -> Result<()> {
    let node = tree
        .get(node_index)
        .cloned()
        .ok_or(LayoutError::InvalidTree)?;

    // If this node isn't dirty for layout, its children don't need a top-down update either,
    // unless a reflow was triggered by a sibling's scrollbar (a very complex edge case).
    // For now, we can safely prune the traversal.
    if node.dirty_flag < DirtyFlag::Layout {
        return Ok(());
    }

    // --- Phase 1: Calculate this node's own used size ---
    // let used_size = calculate_used_size_for_node(tree, node_index, containing_block_size, ...)?;
    let mut used_size = containing_block_size; // STUB

    // --- Phase 2: Layout children using a formatting context ---
    let constraints = LayoutConstraints {
        available_size: used_size, /* ... */
    };
    // let layout_output = layout_formatting_context(ctx, tree, node_index, &constraints)?;
    let layout_output: fc::LayoutOutput = Default::default(); // STUB
    let content_size = layout_output.overflow_size;

    // --- Phase 3: Check for scrollbars ---
    let scrollbar_info = check_scrollbar_necessity(content_size, used_size /* ... */);
    let inner_size_after_scrollbars = LogicalSize::new(
        used_size.width - scrollbar_info.scrollbar_width,
        used_size.height - scrollbar_info.scrollbar_height,
    );

    // If adding scrollbars changed the available space for children, we need another pass.
    if inner_size_after_scrollbars != used_size {
        *reflow_needed_for_scrollbars = true;
        // Re-dirty all children because their containing block has shrunk.
        for &child_index in &node.children {
            tree.mark_subtree_dirty(child_index, DirtyFlag::Layout);
        }
        // We could even re-run layout for this node's children immediately,
        // but for simplicity, we let the main loop handle it.
    }

    // --- Phase 4: Update self and recurse to children ---
    if let Some(n) = tree.get_mut(node_index) {
        n.used_size = Some(used_size);
    }

    for (&child_index, &child_relative_pos) in &layout_output.positions {
        // Update child's absolute position in the final tree
        // positioned_tree.absolute_positions.insert(child_index, absolute_parent_pos +
        // child_relative_pos);

        // Recurse
        calculate_layout_recursively(
            ctx,
            tree,
            child_index,
            used_size,
            reflow_needed_for_scrollbars,
        )?;
    }

    Ok(())
}
