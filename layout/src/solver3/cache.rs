//! Handling Viewport Resizing and Layout Thrashing
//!
//! The viewport size is a fundamental input to the entire layout process.
//! A change in viewport size must trigger a relayout.
//!
//! 1. The `layout_document` function takes the `viewport` as an argument. The `LayoutCache` now
//!    also stores the `viewport` from the previous frame.
//! 2. The `reconcile_and_invalidate` function detects that the viewport has changed size
//! 3. This single change—marking the root as a layout root—forces a full top-down pass
//!    (`calculate_layout_for_subtree` starting from the root). This correctly recalculates all
//!    percentage-based sizes and repositions all elements according to the new viewport dimensions.
//!    The intrinsic size calculation (bottom-up) can often be skipped, as it's independent of the
//!    container size, which is a significant optimization.

use std::collections::{BTreeMap, BTreeSet};

use azul_core::{
    dom::NodeId,
    window::{LogicalPosition, LogicalRect, WritingMode},
};

use crate::{
    solver3::{
        fc::{layout_formatting_context, LayoutConstraints},
        geometry::PositionedRectangle,
        layout_tree::SubtreeHash,
        LayoutContext, LayoutError, LayoutTree, Result,
    },
    text3::cache::{FontLoaderTrait, ParsedFontTrait},
};

/// The persistent cache that holds the layout state between frames.
#[derive(Debug, Clone, Default)]
pub struct LayoutCache {
    /// The fully laid-out tree from the previous frame. This is our primary cache.
    pub tree: Option<LayoutTree>,
    /// The final, absolute positions of all nodes from the previous frame.
    pub absolute_positions: BTreeMap<usize, LogicalPosition>,
    /// The viewport size from the last layout pass, used to detect resizes.
    pub viewport: Option<LogicalRect>,
}

impl LayoutCache {
    /// Constructs a map of positioned rectangles from the cached layout data.
    pub fn get_rectangles(&self) -> BTreeMap<NodeId, PositionedRectangle> {
        let Some(tree) = self.tree.as_ref() else {
            return BTreeMap::new();
        };
        tree.nodes
            .iter()
            .enumerate()
            .filter_map(|(idx, node)| {
                let dom_id = node.dom_node_id?;
                let size = node.used_size?;
                let pos = self.absolute_positions.get(&idx).copied()?;

                Some((
                    dom_id,
                    PositionedRectangle {
                        bounds: LogicalRect::new(pos, size),
                        margin: node.box_props.margin.into(), // Assumes an Into impl
                        border: node.box_props.border.into(),
                        padding: node.box_props.padding.into(),
                    },
                ))
            })
            .collect()
    }

    /// Extracts word positions from the cached layout tree.
    pub fn get_word_positions(&self) -> BTreeMap<NodeId, Vec<LogicalRect>> {
        let Some(tree) = self.tree.as_ref() else {
            return BTreeMap::new();
        };
        // ... implementation to traverse tree and extract from inline_layout_result ...
        BTreeMap::new()
    }
}

/// The result of a reconciliation pass.
#[derive(Debug, Default)]
pub struct ReconciliationResult {
    /// Set of nodes whose intrinsic size needs to be recalculated (bottom-up pass).
    pub intrinsic_dirty: BTreeSet<usize>,
    /// Set of layout roots whose subtrees need a new top-down layout pass.
    pub layout_roots: BTreeSet<usize>,
}

impl ReconciliationResult {
    /// Checks if any layout or paint work is needed.
    pub fn is_clean(&self) -> bool {
        self.intrinsic_dirty.is_empty() && self.layout_roots.is_empty()
    }
}

/// After dirty subtrees are laid out, this repositions their clean siblings
/// without recalculating their internal layout.
pub fn reposition_clean_subtrees(
    tree: &LayoutTree,
    layout_roots: &BTreeSet<usize>,
    absolute_positions: &mut BTreeMap<usize, LogicalPosition>,
) {
    // Find the unique parents of all dirty layout roots. These are the containers
    // where sibling positions need to be adjusted.
    let mut parents_to_reposition = BTreeSet::new();
    for &root_idx in layout_roots {
        if let Some(parent_idx) = tree.get(root_idx).and_then(|n| n.parent) {
            parents_to_reposition.insert(parent_idx);
        }
    }

    for parent_idx in parents_to_reposition {
        let parent_node = match tree.get(parent_idx) {
            Some(n) => n,
            None => continue,
        };

        // NOTE: This is a simplified Block Formatting Context (BFC) repositioning.
        // A full implementation would need to dispatch to a flex, grid, or BFC
        // repositioning algorithm based on the parent's formatting context.

        // Start the pen at the parent's content-box origin.
        let parent_pos = absolute_positions
            .get(&parent_idx)
            .copied()
            .unwrap_or_default();
        let mut pen = LogicalPosition::new(
            parent_pos.x + parent_node.box_props.padding.left,
            parent_pos.y + parent_node.box_props.padding.top,
        );

        for &child_idx in &parent_node.children {
            let child_node = match tree.get(child_idx) {
                Some(n) => n,
                None => continue,
            };

            // Calculate the full margin-box size of the child for stacking.
            let child_size = child_node.used_size.unwrap_or_default();
            let margin_box_height = child_size.height
                + child_node
                    .box_props
                    .margin
                    .main_sum(WritingMode::HorizontalTb);

            if layout_roots.contains(&child_idx) {
                // This child was dirty and has already been repositioned correctly.
                // We just need to update our pen to its final position for the next sibling.
                pen.y =
                    absolute_positions.get(&child_idx).map_or(pen.y, |p| p.y) + margin_box_height;
            } else {
                // This child is CLEAN. Its internal layout is correct, but its
                // absolute position might be wrong. We need to shift it.
                let old_pos = match absolute_positions.get(&child_idx) {
                    Some(p) => *p,
                    None => continue, // Should not happen if cache is consistent
                };

                // The new correct position is our current pen.
                let new_pos = pen;

                if old_pos != new_pos {
                    let delta = LogicalPosition::new(new_pos.x - old_pos.x, new_pos.y - old_pos.y);
                    shift_subtree_position(child_idx, delta, tree, absolute_positions);
                }

                // Advance the pen for the next sibling.
                pen.y += margin_box_height;
            }
        }
    }
}

/// Helper to recursively shift the absolute position of a node and all its descendants.
pub fn shift_subtree_position(
    node_idx: usize,
    delta: LogicalPosition,
    tree: &LayoutTree,
    absolute_positions: &mut BTreeMap<usize, LogicalPosition>,
) {
    if let Some(pos) = absolute_positions.get_mut(&node_idx) {
        pos.x += delta.x;
        pos.y += delta.y;
    }

    if let Some(node) = tree.get(node_idx) {
        for &child_idx in &node.children {
            shift_subtree_position(child_idx, delta, tree, absolute_positions);
        }
    }
}

/// Compares the new DOM against the cached tree, creating a new tree
/// and identifying which parts need to be re-laid out.
pub fn reconcile_and_invalidate<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    cache: &LayoutCache,
    viewport: LogicalRect,
) -> Result<(LayoutTree, ReconciliationResult)> {
    let mut new_tree_builder = LayoutTreeBuilder::new();
    let mut recon_result = ReconciliationResult::default();
    let old_tree = cache.tree.as_ref();

    // Check for viewport resize, which dirties the root for a top-down pass.
    if cache.viewport.map_or(true, |v| v.size != viewport.size) {
        recon_result.layout_roots.insert(0); // Root is always index 0
    }

    let root_dom_id = ctx
        .styled_dom
        .root
        .into_crate_internal()
        .unwrap_or(NodeId::ZERO);
    let root_idx = reconcile_recursive(
        ctx,
        root_dom_id,
        old_tree.map(|t| t.root),
        None,
        old_tree,
        &mut new_tree_builder,
        &mut recon_result,
    )?;

    // Clean up layout roots: if a parent is a layout root, its children don't need to be.
    let final_layout_roots = recon_result
        .layout_roots
        .iter()
        .filter(|&&idx| {
            let mut current = new_tree_builder.get(idx).and_then(|n| n.parent);
            while let Some(p_idx) = current {
                if recon_result.layout_roots.contains(&p_idx) {
                    return false;
                }
                current = new_tree_builder.get(p_idx).and_then(|n| n.parent);
            }
            true
        })
        .copied()
        .collect();
    recon_result.layout_roots = final_layout_roots;

    let new_tree = new_tree_builder.build(root_idx);
    Ok((new_tree, recon_result))
}

/// Recursively traverses the new DOM and old tree, building a new tree and marking dirty nodes.
pub fn reconcile_recursive<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    new_dom_id: NodeId,
    old_tree_idx: Option<usize>,
    new_parent_idx: Option<usize>,
    old_tree: Option<&LayoutTree>,
    new_tree_builder: &mut LayoutTreeBuilder,
    recon: &mut ReconciliationResult,
) -> Result<usize> {
    let old_node = old_tree.and_then(|t| old_tree_idx.and_then(|idx| t.get(idx)));
    let new_node_data_hash = ctx.styled_dom.get_node_data_hash(new_dom_id); // Assumes this helper exists

    // A node is dirty if it's new, or if its data/style hash has changed.
    let is_dirty = old_node.map_or(true, |n| {
        let old_node_data_hash = old_tree.unwrap().get_node_data_hash(n.dom_node_id.unwrap());
        new_node_data_hash != old_node_data_hash
    });

    let new_node_idx = if is_dirty {
        new_tree_builder.create_node_from_dom(ctx.styled_dom, new_dom_id, new_parent_idx)?
    } else {
        new_tree_builder.clone_node_from_old(old_node.unwrap(), new_parent_idx)
    };

    // Reconcile children to check for structural changes and build the new tree structure.
    let new_children_dom_ids: Vec<_> = new_dom_id.children(ctx.styled_dom).collect();
    let old_children_indices: Vec<_> = old_node.map(|n| n.children.clone()).unwrap_or_default();

    let mut children_are_different = new_children_dom_ids.len() != old_children_indices.len();
    let mut new_child_hashes = Vec::new();

    // NOTE: This is a simple list-diffing algorithm. For production, a key-based
    // algorithm (like in React) is necessary to correctly handle reordered items.
    for i in 0..new_children_dom_ids.len() {
        let new_child_dom_id = new_children_dom_ids[i];
        let old_child_idx = old_children_indices.get(i).copied();

        let reconciled_child_idx = reconcile_recursive(
            ctx,
            new_child_dom_id,
            old_child_idx,
            Some(new_node_idx),
            old_tree,
            new_tree_builder,
            recon,
        )?;
        let child_node = new_tree_builder.get(reconciled_child_idx).unwrap();
        new_child_hashes.push(child_node.subtree_hash.0);

        // Check if the reconciled child's subtree is different from the old one.
        if old_tree.and_then(|t| t.get(old_child_idx?).map(|n| n.subtree_hash))
            != Some(child_node.subtree_hash)
        {
            children_are_different = true;
        }
    }

    // After reconciling children, calculate this node's full subtree hash.
    let final_subtree_hash = calculate_subtree_hash(new_node_data_hash, &new_child_hashes);
    let current_node = new_tree_builder.get_mut(new_node_idx).unwrap();
    current_node.subtree_hash = final_subtree_hash;

    // If the node itself was dirty, or its children's structure changed, it's a layout boundary.
    if is_dirty || children_are_different {
        recon.intrinsic_dirty.insert(new_node_idx);
        recon.layout_roots.insert(new_node_idx);
    }

    Ok(new_node_idx)
}

/// Recursive, top-down pass to calculate used sizes and positions for a given subtree.
pub fn calculate_layout_for_subtree<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree,
    node_index: usize,
    // The absolute position of the containing block's content-box origin.
    containing_block_pos: LogicalPosition,
    containing_block_size: LogicalSize,
    // The map of final absolute positions, which is mutated by this function.
    absolute_positions: &mut BTreeMap<usize, LogicalPosition>,
    reflow_needed_for_scrollbars: &mut bool,
) -> Result<()> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
    let dom_id = node.dom_node_id;

    // --- Phase 1: Calculate this node's used size ---
    // This depends on its own CSS properties and the size of its containing block.
    let intrinsic = node.intrinsic_sizes.unwrap_or_default();
    let used_size =
        calculate_used_size_for_node(dom_id, containing_block_size, intrinsic, &node.box_props)?;

    // --- Phase 2: Layout children using a formatting context ---
    let constraints = LayoutConstraints {
        available_size: used_size.inner_size(&node.box_props), // Pass content-box size
        bfc_state: None,                                       /* Simplified for this example
                                                                * ... other constraints */
    };
    let layout_output = layout_formatting_context(ctx, tree, node_index, &constraints)?;
    let content_size = layout_output.overflow_size;

    // --- Phase 3: Check for scrollbars and potential reflow ---
    // let scrollbar_info = check_scrollbar_necessity(content_size, used_size, ...);
    // if scrollbar_info.needs_reflow() { *reflow_needed_for_scrollbars = true; }
    // let inner_size_after_scrollbars = used_size.shrink_by_scrollbars(scrollbar_info);
    let inner_size_after_scrollbars = used_size.inner_size(&node.box_props); // Simplified

    // --- Phase 4: Update self and recurse to children ---
    let current_node = tree.get_mut(node_index).unwrap();
    current_node.used_size = Some(used_size);

    // The absolute position of this node's content-box for its children.
    let self_content_box_pos = LogicalPosition::new(
        containing_block_pos.x + current_node.box_props.padding.left,
        containing_block_pos.y + current_node.box_props.padding.top,
    );

    for (&child_index, &child_relative_pos) in &layout_output.positions {
        let child_node = tree.get_mut(child_index).ok_or(LayoutError::InvalidTree)?;

        // Store the calculated relative position on the child node.
        child_node.relative_position = Some(child_relative_pos);

        // Calculate and store the final absolute position for painting.
        let child_absolute_pos = LogicalPosition::new(
            self_content_box_pos.x + child_relative_pos.x,
            self_content_box_pos.y + child_relative_pos.y,
        );
        absolute_positions.insert(child_index, child_absolute_pos);

        // Recurse into the child's subtree.
        calculate_layout_for_subtree(
            ctx,
            tree,
            child_index,
            child_absolute_pos,
            child_node.used_size.unwrap_or_default(),
            absolute_positions,
            reflow_needed_for_scrollbars,
        )?;
    }

    Ok(())
}

fn calculate_subtree_hash(node_self_hash: u64, child_hashes: &[u64]) -> SubtreeHash {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&node_self_hash, &mut hasher);
    std::hash::Hash::hash(child_hashes, &mut hasher);
    SubtreeHash(std::hash::Hasher::finish(&hasher))
}
