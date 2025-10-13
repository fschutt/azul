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

use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{DefaultHasher, Hash, Hasher},
};

use azul_core::{
    dom::NodeId,
    styled_dom::{StyledDom, StyledNode},
    ui_solver::FormattingContext,
    window::{LogicalPosition, LogicalRect, LogicalSize, WritingMode},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        layout::{LayoutFlexWrap, LayoutJustifyContent, LayoutOverflow, LayoutWrap, LayoutWritingMode},
        property::{CssProperty, CssPropertyType},
        style::StyleTextAlign,
    },
    LayoutDebugMessage,
};

use crate::{
    solver3::{
        fc::{self, layout_formatting_context, LayoutConstraints, OverflowBehavior}, geometry::{CssSize, PositionedRectangle}, getters::{get_justify_content, get_overflow_x, get_overflow_y, get_text_align, get_wrap, get_writing_mode}, layout_tree::{LayoutNode, LayoutTreeBuilder, SubtreeHash}, LayoutContext, LayoutError, LayoutTree, Result
    },
    text3::{
        self,
        cache::{FontLoaderTrait, ParsedFontTrait},
    },
};

/// Convert LayoutWritingMode to WritingMode
pub fn to_writing_mode(wm: LayoutWritingMode) -> WritingMode {
    match wm {
        LayoutWritingMode::HorizontalTb => WritingMode::HorizontalTb,
        LayoutWritingMode::VerticalRl => WritingMode::VerticalRl,
        LayoutWritingMode::VerticalLr => WritingMode::VerticalLr,
    }
}

/// Convert LayoutOverflow to OverflowBehavior
fn to_overflow_behavior(overflow: LayoutOverflow) -> fc::OverflowBehavior {
    match overflow {
        LayoutOverflow::Visible => fc::OverflowBehavior::Visible,
        LayoutOverflow::Hidden | LayoutOverflow::Clip => fc::OverflowBehavior::Hidden,
        LayoutOverflow::Scroll => fc::OverflowBehavior::Scroll,
        LayoutOverflow::Auto => fc::OverflowBehavior::Auto,
    }
}

/// The persistent cache that holds the layout state between frames.
#[derive(Debug, Clone, Default)]
pub struct LayoutCache<T: ParsedFontTrait> {
    /// The fully laid-out tree from the previous frame. This is our primary cache.
    pub tree: Option<LayoutTree<T>>,
    /// The final, absolute positions of all nodes from the previous frame.
    pub absolute_positions: BTreeMap<usize, LogicalPosition>,
    /// The viewport size from the last layout pass, used to detect resizes.
    pub viewport: Option<LogicalRect>,
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
/// without recalculating their internal layout. This is a critical optimization.
///
/// This function acts as a dispatcher, inspecting the parent's formatting context
/// and calling the appropriate repositioning algorithm. For complex layout modes
/// like Flexbox or Grid, this optimization is skipped, as a full relayout is
/// often required to correctly recalculate spacing and sizing for all siblings.
pub fn reposition_clean_subtrees<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    tree: &LayoutTree<T>,
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

        // Dispatch to the correct repositioning logic based on the parent's layout mode.
        match parent_node.formatting_context {
            // Cases that use simple block-flow stacking can be optimized.
            FormattingContext::Block { .. } | FormattingContext::TableRowGroup => {
                reposition_block_flow_siblings(
                    styled_dom,
                    parent_idx,
                    parent_node,
                    tree,
                    layout_roots,
                    absolute_positions,
                );
            }

            FormattingContext::Flex | FormattingContext::Grid => {
                // Taffy handles this, so if a child is dirty, the parent would have already
                // been marked as a layout_root and re-laid out by Taffy.
                // We do nothing here for Flex or Grid.
            }

            FormattingContext::Table | FormattingContext::TableRow => {
                // STUB: Table layout is interdependent. A change in one cell's size
                // can affect the entire column's width or row's height, requiring a
                // full relayout of the table. This optimization is skipped.
            }

            // Other contexts either don't contain children in a way that this
            // optimization applies (e.g., Inline, TableCell) or are handled by other
            // layout mechanisms (e.g., OutOfFlow).
            _ => { /* Do nothing */ }
        }
    }
}

/// Checks if a flex container is simple enough to be treated like a block-stack for repositioning.
fn is_simple_flex_stack(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> bool {
    let Some(id) = dom_id else { return false };
    let binding = styled_dom.styled_nodes.as_container();
    let styled_node = match binding.get(id) {
        Some(styled_node) => styled_node,
        None => return false,
    };

    // Must be a single-line flex container
    let wrap = get_wrap(styled_dom, id, &styled_node.state);

    if wrap != LayoutFlexWrap::NoWrap {
        return false;
    }

    // Must be start-aligned, so there's no space distribution to recalculate.
    let justify = get_justify_content(styled_dom, id, &styled_node.state);

    if !matches!(
        justify,
        LayoutJustifyContent::FlexStart | LayoutJustifyContent::Start
    ) {
        return false;
    }

    // Crucially, no clean siblings can have flexible sizes, otherwise a dirty
    // sibling's size change could affect their resolved size.
    // NOTE: This check is expensive and incomplete. A more robust solution might
    // store flags on the LayoutNode indicating if flex factors are present.
    // For now, we assume that if a container *could* have complex flex behavior,
    // we play it safe and require a full relayout. This heuristic is a compromise.
    // To be truly safe, we'd have to check all children for flex-grow/shrink > 0.

    true
}

/// Repositions clean children within a simple block-flow layout (like a BFC or a table-row-group).
/// It stacks children along the main axis, preserving their previously calculated cross-axis
/// alignment.
fn reposition_block_flow_siblings<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    parent_idx: usize,
    parent_node: &LayoutNode<T>,
    tree: &LayoutTree<T>,
    layout_roots: &BTreeSet<usize>,
    absolute_positions: &mut BTreeMap<usize, LogicalPosition>,
) {
    let dom_id = parent_node.dom_node_id.unwrap_or(NodeId::ZERO);
    let styled_node_state = styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| n.state.clone())
        .unwrap_or_default();
    let writing_mode = to_writing_mode(get_writing_mode(styled_dom, dom_id, &styled_node_state));
    let parent_pos = absolute_positions
        .get(&parent_idx)
        .copied()
        .unwrap_or_default();
    let content_box_origin = LogicalPosition::new(
        parent_pos.x + parent_node.box_props.padding.left,
        parent_pos.y + parent_node.box_props.padding.top,
    );

    let mut main_pen = 0.0;

    for &child_idx in &parent_node.children {
        let child_node = match tree.get(child_idx) {
            Some(n) => n,
            None => continue,
        };

        let child_size = child_node.used_size.unwrap_or_default();
        let margin_box_main_size =
            child_size.main(writing_mode) + child_node.box_props.margin.main_sum(writing_mode);

        if layout_roots.contains(&child_idx) {
            // This child was DIRTY and has been correctly repositioned.
            // Update the pen to the position immediately after this child.
            let new_pos = match absolute_positions.get(&child_idx) {
                Some(p) => *p,
                None => continue,
            };

            let main_axis_offset = match writing_mode {
                WritingMode::HorizontalTb => new_pos.y - content_box_origin.y,
                WritingMode::VerticalRl | WritingMode::VerticalLr => {
                    new_pos.x - content_box_origin.x
                }
            };

            main_pen = main_axis_offset
                + child_size.main(writing_mode)
                + child_node.box_props.margin.main_end(writing_mode);
        } else {
            // This child is CLEAN. Calculate its new position and shift its entire subtree.
            let old_pos = match absolute_positions.get(&child_idx) {
                Some(p) => *p,
                None => continue,
            };

            let new_main_pos = main_pen + child_node.box_props.margin.main_start(writing_mode);
            let old_relative_pos = child_node.relative_position.unwrap_or_default();
            let cross_pos = match writing_mode {
                WritingMode::HorizontalTb => old_relative_pos.x,
                WritingMode::VerticalRl | WritingMode::VerticalLr => old_relative_pos.y,
            };
            let new_relative_pos =
                LogicalPosition::from_main_cross(new_main_pos, cross_pos, writing_mode);
            let new_absolute_pos = LogicalPosition::new(
                content_box_origin.x + new_relative_pos.x,
                content_box_origin.y + new_relative_pos.y,
            );

            if old_pos != new_absolute_pos {
                let delta = LogicalPosition::new(
                    new_absolute_pos.x - old_pos.x,
                    new_absolute_pos.y - old_pos.y,
                );
                shift_subtree_position(child_idx, delta, tree, absolute_positions);
            }

            main_pen += margin_box_main_size;
        }
    }
}

/// Helper to recursively shift the absolute position of a node and all its descendants.
pub fn shift_subtree_position<T: ParsedFontTrait>(
    node_idx: usize,
    delta: LogicalPosition,
    tree: &LayoutTree<T>,
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
    cache: &LayoutCache<T>,
    viewport: LogicalRect,
) -> Result<(LayoutTree<T>, ReconciliationResult)> {
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
    old_tree: Option<&LayoutTree<T>>,
    new_tree_builder: &mut LayoutTreeBuilder<T>,
    recon: &mut ReconciliationResult,
) -> Result<usize> {
    let old_node = old_tree.and_then(|t| old_tree_idx.and_then(|idx| t.get(idx)));
    let new_node_data_hash = hash_styled_node_data(&ctx.styled_dom, new_dom_id);

    // A node is dirty if it's new, or if its data/style hash has changed.

    let is_dirty = old_node.map_or(true, |n| new_node_data_hash != n.node_data_hash);

    let new_node_idx = if is_dirty {
        new_tree_builder.create_node_from_dom(ctx.styled_dom, new_dom_id, new_parent_idx)?
    } else {
        new_tree_builder.clone_node_from_old(old_node.unwrap(), new_parent_idx)
    };

    // Reconcile children to check for structural changes and build the new tree structure.
    let hierarchy_container = ctx.styled_dom.node_hierarchy.as_container();
    let new_children_dom_ids: Vec<_> = {
        let mut children = Vec::new();
        if let Some(hierarchy_item) = hierarchy_container.get(new_dom_id) {
            if let Some(mut child_id) = hierarchy_item.first_child_id(new_dom_id) {
                children.push(child_id);
                while let Some(hierarchy_item) = hierarchy_container.get(child_id) {
                    if let Some(next) = hierarchy_item.next_sibling_id() {
                        children.push(next);
                        child_id = next;
                    } else {
                        break;
                    }
                }
            }
        }
        children
    };
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
/// This is the single, authoritative function for in-flow layout.
pub fn calculate_layout_for_subtree<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    node_index: usize,
    // The absolute position of the containing block's content-box origin.
    containing_block_pos: LogicalPosition,
    containing_block_size: LogicalSize,
    // The map of final absolute positions, which is mutated by this function.
    absolute_positions: &mut BTreeMap<usize, LogicalPosition>,
    reflow_needed_for_scrollbars: &mut bool,
) -> Result<()> {
    let (constraints, dom_id, writing_mode, mut final_used_size, box_props) = {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        let dom_id = node.dom_node_id.ok_or(LayoutError::InvalidTree)?;

        // --- Phase 1: Calculate this node's PROVISIONAL used size ---
        // This size is based on the node's CSS properties (width, height, etc.) and
        // its containing block. If height is 'auto', this is a temporary value.
        let intrinsic = node.intrinsic_sizes.clone().unwrap_or_default();
        let mut final_used_size = crate::solver3::sizing::calculate_used_size_for_node(
            ctx.styled_dom,
            Some(dom_id),
            containing_block_size,
            intrinsic,
            &node.box_props,
        )?;

        // --- Phase 2: Layout children using a formatting context ---

        // Fetch the writing mode for the current context.
        let styled_node_state = ctx
            .styled_dom
            .styled_nodes
            .as_container()
            .get(dom_id)
            .map(|n| n.state.clone())
            .unwrap_or_default();
        let layout_writing_mode = get_writing_mode(ctx.styled_dom, dom_id, &styled_node_state); // This should come from the node's style.
        let writing_mode = to_writing_mode(layout_writing_mode);
        let text_align = get_text_align(ctx.styled_dom, dom_id, &styled_node_state);

        let constraints = LayoutConstraints {
            available_size: node.box_props.inner_size(final_used_size, writing_mode),
            bfc_state: None,
            writing_mode,
            text_align: match text_align {
                StyleTextAlign::Start | StyleTextAlign::Left => fc::TextAlign::Start,
                StyleTextAlign::End | StyleTextAlign::Right => fc::TextAlign::End,
                StyleTextAlign::Center => fc::TextAlign::Center,
                StyleTextAlign::Justify => fc::TextAlign::Justify,}
        };

        (
            constraints,
            dom_id,
            writing_mode,
            final_used_size,
            node.box_props.clone(),
        )
    };

    let layout_output = layout_formatting_context(ctx, tree, text_cache, node_index, &constraints)?;
    let content_size = layout_output.overflow_size;

    // --- Phase 2.5: Resolve 'auto' main-axis size ---

    // If the node's main-axis size depends on its content, we update its used size now.
    if crate::solver3::sizing::get_css_height(ctx.styled_dom, Some(dom_id)) == CssSize::Auto {
        let node_props = &tree.get(node_index).unwrap().box_props;
        let main_axis_padding_border =
            node_props.padding.main_sum(writing_mode) + node_props.border.main_sum(writing_mode);

        let new_main_size = content_size.main(writing_mode) + main_axis_padding_border;
        final_used_size = final_used_size.with_main(writing_mode, new_main_size);
    }

    // --- Phase 3: Check for scrollbars and potential reflow ---
    let styled_node_state = ctx
        .styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| n.state.clone())
        .unwrap_or_default();
    
    let overflow_x = get_overflow_x(ctx.styled_dom, dom_id, &styled_node_state);
    let overflow_y = get_overflow_y(ctx.styled_dom, dom_id, &styled_node_state);

    let scrollbar_info = fc::check_scrollbar_necessity(
        content_size,
        box_props.inner_size(final_used_size, writing_mode),
        to_overflow_behavior(overflow_x),
        to_overflow_behavior(overflow_y),
    );

    if scrollbar_info.needs_reflow() {
        *reflow_needed_for_scrollbars = true;
        return Ok(());
    }

    let content_box_size = box_props.inner_size(final_used_size, writing_mode);
    let inner_size_after_scrollbars = scrollbar_info.shrink_size(content_box_size);

    // --- Phase 4: Update self and recurse to children ---
    let current_node = tree.get_mut(node_index).unwrap();
    current_node.used_size = Some(final_used_size);

    // The absolute position of this node's content-box for its children.
    let self_content_box_pos = LogicalPosition::new(
        containing_block_pos.x + current_node.box_props.padding.left,
        containing_block_pos.y + current_node.box_props.padding.top,
    );

    for (&child_index, &child_relative_pos) in &layout_output.positions {
        let child_node = tree.get_mut(child_index).ok_or(LayoutError::InvalidTree)?;
        child_node.relative_position = Some(child_relative_pos);

        let child_absolute_pos = LogicalPosition::new(
            self_content_box_pos.x + child_relative_pos.x,
            self_content_box_pos.y + child_relative_pos.y,
        );
        absolute_positions.insert(child_index, child_absolute_pos);

        calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            child_index,
            self_content_box_pos,
            inner_size_after_scrollbars,
            absolute_positions,
            reflow_needed_for_scrollbars,
        )?;
    }
    Ok(())
}

fn hash_styled_node_data(dom: &StyledDom, node_id: NodeId) -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Some(styled_node) = dom.styled_nodes.as_container().get(node_id) {
        styled_node.state.hash(&mut hasher);
    }
    if let Some(node_data) = dom.node_data.as_container().get(node_id) {
        node_data.get_node_type().hash(&mut hasher);
    }
    hasher.finish()
}

fn calculate_subtree_hash(node_self_hash: u64, child_hashes: &[u64]) -> SubtreeHash {
    let mut hasher = DefaultHasher::new();
    node_self_hash.hash(&mut hasher);
    child_hashes.hash(&mut hasher);
    SubtreeHash(hasher.finish())
}
