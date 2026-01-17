//! Handling Viewport Resizing and Layout Thrashing
//!
//! The viewport size is a fundamental input to the entire layout process.
//! A change in viewport size must trigger a relayout.
//!
//! 1. The `layout_document` function takes the `viewport` as an argument. The `LayoutCache` stores
//!    the `viewport` from the previous frame.
//! 2. The `reconcile_and_invalidate` function detects that the viewport has changed size
//! 3. This single change—marking the root as a layout root—forces a full top-down pass
//!    (`calculate_layout_for_subtree` starting from the root). This correctly recalculates
//!    all(`calculate_layout_for_subtree` starting from the root). This correctly recalculates all
//!    percentage-based sizes and repositions all elements according to the new viewport dimensions.
//! 4. The intrinsic size calculation (bottom-up) can often be skipped, as it's independent of the
//!    container size, which is a significant optimization.

use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{DefaultHasher, Hash, Hasher},
};

use azul_core::{
    dom::{FormattingContext, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    styled_dom::{StyledDom, StyledNode},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        layout::{
            LayoutDisplay, LayoutFlexWrap, LayoutHeight, LayoutJustifyContent, LayoutOverflow,
            LayoutPosition, LayoutWrap, LayoutWritingMode,
        },
        property::{CssProperty, CssPropertyType},
        style::StyleTextAlign,
    },
    LayoutDebugMessage, LayoutDebugMessageType,
};

use crate::{
    font_traits::{FontLoaderTrait, ParsedFontTrait, TextLayoutCache},
    solver3::{
        fc::{self, layout_formatting_context, LayoutConstraints, OverflowBehavior},
        geometry::PositionedRectangle,
        getters::{
            get_css_height, get_display_property, get_justify_content, get_overflow_x,
            get_overflow_y, get_text_align, get_wrap, get_writing_mode, MultiValue,
        },
        layout_tree::{
            is_block_level, AnonymousBoxType, LayoutNode, LayoutTreeBuilder, SubtreeHash,
        },
        positioning::get_position_type,
        scrollbar::ScrollbarRequirements,
        sizing::calculate_used_size_for_node,
        LayoutContext, LayoutError, LayoutTree, Result,
    },
    text3::cache::AvailableSpace as Text3AvailableSpace,
};

/// The persistent cache that holds the layout state between frames.
#[derive(Debug, Clone, Default)]
pub struct LayoutCache {
    /// The fully laid-out tree from the previous frame. This is our primary cache.
    pub tree: Option<LayoutTree>,
    /// The final, absolute positions of all nodes from the previous frame.
    pub calculated_positions: BTreeMap<usize, LogicalPosition>,
    /// The viewport size from the last layout pass, used to detect resizes.
    pub viewport: Option<LogicalRect>,
    /// Stable scroll IDs computed from node_data_hash (layout index -> scroll ID)
    pub scroll_ids: BTreeMap<usize, u64>,
    /// Mapping from scroll ID to DOM NodeId for hit testing
    pub scroll_id_to_node_id: BTreeMap<u64, NodeId>,
    /// CSS counter values for each node and counter name.
    /// Key: (layout_index, counter_name), Value: counter value
    /// This stores the computed counter values after processing counter-reset and
    /// counter-increment.
    pub counters: BTreeMap<(usize, String), i32>,
    /// Cache of positioned floats for each BFC node (layout_index -> FloatingContext).
    /// This persists float positions across multiple layout passes, ensuring IFC
    /// children always have access to correct float exclusions even when layout is
    /// recalculated.
    pub float_cache: BTreeMap<usize, fc::FloatingContext>,
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
pub fn reposition_clean_subtrees(
    styled_dom: &StyledDom,
    tree: &LayoutTree,
    layout_roots: &BTreeSet<usize>,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
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
                    calculated_positions,
                );
            }

            FormattingContext::Flex | FormattingContext::Grid => {
                // Taffy handles this, so if a child is dirty, the parent would have
                // already been marked as a layout_root and re-laid out by Taffy.
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

/// Convert LayoutOverflow to OverflowBehavior
pub fn to_overflow_behavior(overflow: MultiValue<LayoutOverflow>) -> fc::OverflowBehavior {
    match overflow.unwrap_or_default() {
        LayoutOverflow::Visible => fc::OverflowBehavior::Visible,
        LayoutOverflow::Hidden | LayoutOverflow::Clip => fc::OverflowBehavior::Hidden,
        LayoutOverflow::Scroll => fc::OverflowBehavior::Scroll,
        LayoutOverflow::Auto => fc::OverflowBehavior::Auto,
    }
}

/// Convert StyleTextAlign to fc::TextAlign
pub const fn style_text_align_to_fc(text_align: StyleTextAlign) -> fc::TextAlign {
    match text_align {
        StyleTextAlign::Start | StyleTextAlign::Left => fc::TextAlign::Start,
        StyleTextAlign::End | StyleTextAlign::Right => fc::TextAlign::End,
        StyleTextAlign::Center => fc::TextAlign::Center,
        StyleTextAlign::Justify => fc::TextAlign::Justify,
    }
}

/// Collects DOM child IDs from the node hierarchy into a Vec.
///
/// This is a helper function that flattens the sibling iteration into a simple loop.
pub fn collect_children_dom_ids(styled_dom: &StyledDom, parent_dom_id: NodeId) -> Vec<NodeId> {
    let hierarchy_container = styled_dom.node_hierarchy.as_container();
    let mut children = Vec::new();

    let Some(hierarchy_item) = hierarchy_container.get(parent_dom_id) else {
        return children;
    };

    let Some(mut child_id) = hierarchy_item.first_child_id(parent_dom_id) else {
        return children;
    };

    children.push(child_id);
    while let Some(hierarchy_item) = hierarchy_container.get(child_id) {
        let Some(next) = hierarchy_item.next_sibling_id() else {
            break;
        };
        children.push(next);
        child_id = next;
    }

    children
}

/// Checks if a flex container is simple enough to be treated like a block-stack for
/// repositioning.
pub fn is_simple_flex_stack(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> bool {
    let Some(id) = dom_id else { return false };
    let binding = styled_dom.styled_nodes.as_container();
    let styled_node = match binding.get(id) {
        Some(styled_node) => styled_node,
        None => return false,
    };

    // Must be a single-line flex container
    let wrap = get_wrap(styled_dom, id, &styled_node.styled_node_state);

    if wrap.unwrap_or_default() != LayoutFlexWrap::NoWrap {
        return false;
    }

    // Must be start-aligned, so there's no space distribution to recalculate.
    let justify = get_justify_content(styled_dom, id, &styled_node.styled_node_state);

    if !matches!(
        justify.unwrap_or_default(),
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

/// Repositions clean children within a simple block-flow layout (like a BFC or a
/// table-row-group). It stacks children along the main axis, preserving their
/// previously calculated cross-axis alignment.
pub fn reposition_block_flow_siblings(
    styled_dom: &StyledDom,
    parent_idx: usize,
    parent_node: &LayoutNode,
    tree: &LayoutTree,
    layout_roots: &BTreeSet<usize>,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
) {
    let dom_id = parent_node.dom_node_id.unwrap_or(NodeId::ZERO);
    let styled_node_state = styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| n.styled_node_state.clone())
        .unwrap_or_default();

    let writing_mode = get_writing_mode(styled_dom, dom_id, &styled_node_state).unwrap_or_default();

    let parent_pos = calculated_positions
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
        let child_main_sum = child_node.box_props.margin.main_sum(writing_mode);
        let margin_box_main_size = child_size.main(writing_mode) + child_main_sum;

        if layout_roots.contains(&child_idx) {
            // This child was DIRTY and has been correctly repositioned.
            // Update the pen to the position immediately after this child.
            let new_pos = match calculated_positions.get(&child_idx) {
                Some(p) => *p,
                None => continue,
            };

            let main_axis_offset = if writing_mode.is_vertical() {
                new_pos.x - content_box_origin.x
            } else {
                new_pos.y - content_box_origin.y
            };

            main_pen = main_axis_offset
                + child_size.main(writing_mode)
                + child_node.box_props.margin.main_end(writing_mode);
        } else {
            // This child is *clean*. Calculate its new position and shift its
            // entire subtree.
            let old_pos = match calculated_positions.get(&child_idx) {
                Some(p) => *p,
                None => continue,
            };

            let child_main_start = child_node.box_props.margin.main_start(writing_mode);
            let new_main_pos = main_pen + child_main_start;
            let old_relative_pos = child_node.relative_position.unwrap_or_default();
            let cross_pos = if writing_mode.is_vertical() {
                old_relative_pos.y
            } else {
                old_relative_pos.x
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
                shift_subtree_position(child_idx, delta, tree, calculated_positions);
            }

            main_pen += margin_box_main_size;
        }
    }
}

/// Helper to recursively shift the absolute position of a node and all its descendants.
pub fn shift_subtree_position(
    node_idx: usize,
    delta: LogicalPosition,
    tree: &LayoutTree,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
) {
    if let Some(pos) = calculated_positions.get_mut(&node_idx) {
        pos.x += delta.x;
        pos.y += delta.y;
    }

    if let Some(node) = tree.get(node_idx) {
        for &child_idx in &node.children {
            shift_subtree_position(child_idx, delta, tree, calculated_positions);
        }
    }
}

/// Compares the new DOM against the cached tree, creating a new tree
/// and identifying which parts need to be re-laid out.
pub fn reconcile_and_invalidate<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
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
        ctx.styled_dom,
        root_dom_id,
        old_tree.map(|t| t.root),
        None,
        old_tree,
        &mut new_tree_builder,
        &mut recon_result,
        &mut ctx.debug_messages,
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
pub fn reconcile_recursive(
    styled_dom: &StyledDom,
    new_dom_id: NodeId,
    old_tree_idx: Option<usize>,
    new_parent_idx: Option<usize>,
    old_tree: Option<&LayoutTree>,
    new_tree_builder: &mut LayoutTreeBuilder,
    recon: &mut ReconciliationResult,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<usize> {
    let node_data = &styled_dom.node_data.as_container()[new_dom_id];

    let old_node = old_tree.and_then(|t| old_tree_idx.and_then(|idx| t.get(idx)));
    let new_node_data_hash = hash_styled_node_data(styled_dom, new_dom_id);

    // A node is dirty if it's new, or if its data/style hash has changed.

    let is_dirty = old_node.map_or(true, |n| new_node_data_hash != n.node_data_hash);

    let new_node_idx = if is_dirty {
        new_tree_builder.create_node_from_dom(
            styled_dom,
            new_dom_id,
            new_parent_idx,
            debug_messages,
        )?
    } else {
        new_tree_builder.clone_node_from_old(old_node.unwrap(), new_parent_idx)
    };

    // CRITICAL: For list-items, create a ::marker pseudo-element as the first child
    // This must be done after the node is created but before processing children
    // Per CSS Lists Module Level 3, ::marker is generated as the first child of list-items
    {
        let node_data = &styled_dom.node_data.as_container()[new_dom_id];
        let node_state = &styled_dom.styled_nodes.as_container()[new_dom_id].styled_node_state;
        let cache = &styled_dom.css_property_cache.ptr;

        let display = cache
            .get_display(node_data, &new_dom_id, node_state)
            .and_then(|v| v.get_property().copied());

        if matches!(display, Some(LayoutDisplay::ListItem)) {
            // Create ::marker pseudo-element for this list-item
            new_tree_builder.create_marker_pseudo_element(styled_dom, new_dom_id, new_node_idx);
        }
    }

    // Reconcile children to check for structural changes and build the new tree structure.
    let new_children_dom_ids: Vec<_> = collect_children_dom_ids(styled_dom, new_dom_id);
    let old_children_indices: Vec<_> = old_node.map(|n| n.children.clone()).unwrap_or_default();

    let mut children_are_different = new_children_dom_ids.len() != old_children_indices.len();
    let mut new_child_hashes = Vec::new();

    // CSS 2.2 Section 9.2.1.1: Anonymous Block Boxes
    // "When an inline box contains an in-flow block-level box, the inline box
    // (and its inline ancestors within the same line box) are broken around
    // the block-level box [...], splitting the inline box into two boxes"
    //
    // When a block container has mixed block/inline children, we must:
    // 1. Wrap consecutive inline children in anonymous block boxes
    // 2. Leave block-level children as direct children

    let has_block_child = new_children_dom_ids
        .iter()
        .any(|&id| is_block_level(styled_dom, id));

    if !has_block_child {
        // All children are inline - no anonymous boxes needed
        // Simple case: process each child directly
        for (i, &new_child_dom_id) in new_children_dom_ids.iter().enumerate() {
            let old_child_idx = old_children_indices.get(i).copied();

            let reconciled_child_idx = reconcile_recursive(
                styled_dom,
                new_child_dom_id,
                old_child_idx,
                Some(new_node_idx),
                old_tree,
                new_tree_builder,
                recon,
                debug_messages,
            )?;
            if let Some(child_node) = new_tree_builder.get(reconciled_child_idx) {
                new_child_hashes.push(child_node.subtree_hash.0);
            }

            if old_tree.and_then(|t| t.get(old_child_idx?).map(|n| n.subtree_hash))
                != new_tree_builder
                    .get(reconciled_child_idx)
                    .map(|n| n.subtree_hash)
            {
                children_are_different = true;
            }
        }
    } else {
        // Mixed content: block and inline children
        // We must create anonymous block boxes around consecutive inline runs

        if let Some(msgs) = debug_messages.as_mut() {
            msgs.push(LayoutDebugMessage::info(format!(
                "[reconcile_recursive] Mixed content in node {}: creating anonymous IFC wrappers",
                new_dom_id.index()
            )));
        }

        let mut inline_run: Vec<(usize, NodeId)> = Vec::new(); // (dom_child_index, dom_id)

        for (i, &new_child_dom_id) in new_children_dom_ids.iter().enumerate() {
            if is_block_level(styled_dom, new_child_dom_id) {
                // End current inline run if any
                if !inline_run.is_empty() {
                    // Create anonymous IFC wrapper for the inline run
                    // This wrapper establishes an Inline Formatting Context
                    let anon_idx = new_tree_builder.create_anonymous_node(
                        new_node_idx,
                        AnonymousBoxType::InlineWrapper,
                        FormattingContext::Inline, // IFC for inline content
                    );

                    if let Some(msgs) = debug_messages.as_mut() {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "[reconcile_recursive] Created anonymous IFC wrapper (layout_idx={}) for {} inline children: {:?}",
                            anon_idx,
                            inline_run.len(),
                            inline_run.iter().map(|(_, id)| id.index()).collect::<Vec<_>>()
                        )));
                    }

                    // Process each inline child under the anonymous wrapper
                    for (pos, inline_dom_id) in inline_run.drain(..) {
                        let old_child_idx = old_children_indices.get(pos).copied();
                        let reconciled_child_idx = reconcile_recursive(
                            styled_dom,
                            inline_dom_id,
                            old_child_idx,
                            Some(anon_idx), // Parent is the anonymous wrapper
                            old_tree,
                            new_tree_builder,
                            recon,
                            debug_messages,
                        )?;
                        if let Some(child_node) = new_tree_builder.get(reconciled_child_idx) {
                            new_child_hashes.push(child_node.subtree_hash.0);
                        }
                    }

                    // Mark anonymous wrapper as dirty for layout
                    recon.intrinsic_dirty.insert(anon_idx);
                    children_are_different = true;
                }

                // Process block-level child directly under parent
                let old_child_idx = old_children_indices.get(i).copied();
                let reconciled_child_idx = reconcile_recursive(
                    styled_dom,
                    new_child_dom_id,
                    old_child_idx,
                    Some(new_node_idx),
                    old_tree,
                    new_tree_builder,
                    recon,
                    debug_messages,
                )?;
                if let Some(child_node) = new_tree_builder.get(reconciled_child_idx) {
                    new_child_hashes.push(child_node.subtree_hash.0);
                }

                if old_tree.and_then(|t| t.get(old_child_idx?).map(|n| n.subtree_hash))
                    != new_tree_builder
                        .get(reconciled_child_idx)
                        .map(|n| n.subtree_hash)
                {
                    children_are_different = true;
                }
            } else {
                // Inline-level child - add to current run
                inline_run.push((i, new_child_dom_id));
            }
        }

        // Process any remaining inline run at the end
        if !inline_run.is_empty() {
            let anon_idx = new_tree_builder.create_anonymous_node(
                new_node_idx,
                AnonymousBoxType::InlineWrapper,
                FormattingContext::Inline, // IFC for inline content
            );

            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[reconcile_recursive] Created trailing anonymous IFC wrapper (layout_idx={}) for {} inline children: {:?}",
                    anon_idx,
                    inline_run.len(),
                    inline_run.iter().map(|(_, id)| id.index()).collect::<Vec<_>>()
                )));
            }

            for (pos, inline_dom_id) in inline_run.drain(..) {
                let old_child_idx = old_children_indices.get(pos).copied();
                let reconciled_child_idx = reconcile_recursive(
                    styled_dom,
                    inline_dom_id,
                    old_child_idx,
                    Some(anon_idx),
                    old_tree,
                    new_tree_builder,
                    recon,
                    debug_messages,
                )?;
                if let Some(child_node) = new_tree_builder.get(reconciled_child_idx) {
                    new_child_hashes.push(child_node.subtree_hash.0);
                }
            }

            recon.intrinsic_dirty.insert(anon_idx);
            children_are_different = true;
        }
    }

    // After reconciling children, calculate this node's full subtree hash.
    let final_subtree_hash = calculate_subtree_hash(new_node_data_hash, &new_child_hashes);
    if let Some(current_node) = new_tree_builder.get_mut(new_node_idx) {
        current_node.subtree_hash = final_subtree_hash;
    }

    // If the node itself was dirty, or its children's structure changed, it's a layout boundary.
    if is_dirty || children_are_different {
        recon.intrinsic_dirty.insert(new_node_idx);
        recon.layout_roots.insert(new_node_idx);
    }

    Ok(new_node_idx)
}

/// Result of `prepare_layout_context`: contains the layout constraints and
/// intermediate values needed for `calculate_layout_for_subtree`.
struct PreparedLayoutContext<'a> {
    constraints: LayoutConstraints<'a>,
    /// DOM ID for the node. None for anonymous boxes.
    dom_id: Option<NodeId>,
    writing_mode: LayoutWritingMode,
    final_used_size: LogicalSize,
    box_props: crate::solver3::geometry::BoxProps,
}

/// Prepares the layout context for a single node by calculating its used size
/// and building the layout constraints for its children.
///
/// For anonymous boxes (no dom_node_id), we use default values and inherit
/// from the containing block.
fn prepare_layout_context<'a, T: ParsedFontTrait>(
    ctx: &LayoutContext<'a, T>,
    node: &LayoutNode,
    containing_block_size: LogicalSize,
) -> Result<PreparedLayoutContext<'a>> {
    let dom_id = node.dom_node_id; // Can be None for anonymous boxes

    // Phase 1: Calculate this node's provisional used size

    // This size is based on the node's CSS properties (width, height, etc.) and
    // its containing block. If height is 'auto', this is a temporary value.
    let intrinsic = node.intrinsic_sizes.clone().unwrap_or_default();
    let final_used_size = calculate_used_size_for_node(
        ctx.styled_dom,
        dom_id, // Now Option<NodeId>
        containing_block_size,
        intrinsic,
        &node.box_props,
    )?;

    // Phase 2: Layout children using a formatting context

    // Fetch the writing mode for the current context.
    // For anonymous boxes, use default values
    let styled_node_state = dom_id
        .and_then(|id| ctx.styled_dom.styled_nodes.as_container().get(id).cloned())
        .map(|n| n.styled_node_state)
        .unwrap_or_default();

    // This should come from the node's style. For anonymous boxes, use defaults.
    let writing_mode = match dom_id {
        Some(id) => get_writing_mode(ctx.styled_dom, id, &styled_node_state).unwrap_or_default(),
        None => LayoutWritingMode::default(),
    };
    let text_align = match dom_id {
        Some(id) => get_text_align(ctx.styled_dom, id, &styled_node_state).unwrap_or_default(),
        None => StyleTextAlign::default(),
    };

    // IMPORTANT: For the available_size that we pass to children, we need to use
    // the containing_block_size if the current node's height is 'auto'.
    // Otherwise, we would pass 0 as available height to children, which breaks
    // table layout and other auto-height containers.
    // For anonymous boxes, assume 'auto' height behavior.
    let css_height: MultiValue<LayoutHeight> = match dom_id {
        Some(id) => get_css_height(ctx.styled_dom, id, &styled_node_state),
        None => MultiValue::Auto, // Anonymous boxes have auto height
    };

    // Get display type to determine sizing behavior
    let display = match dom_id {
        Some(id) => get_display_property(ctx.styled_dom, Some(id)),
        None => MultiValue::Auto, // Anonymous boxes behave like blocks
    };

    let available_size_for_children = if should_use_content_height(&css_height) {
        // Height is auto - use containing block size as available size
        let inner_size = node.box_props.inner_size(final_used_size, writing_mode);

        // For inline elements (display: inline), the available width comes from
        // the containing block, not from the element's own intrinsic size.
        // CSS 2.2 § 10.3.1: Inline, non-replaced elements use containing block width.
        // The containing_block_size already has parent's padding subtracted when
        // passed from the parent's layout (via inner_size calculation).
        let available_width = match display {
            MultiValue::Exact(LayoutDisplay::Inline) | MultiValue::Auto => {
                // Inline elements flow within the containing block
                containing_block_size.width
            }
            _ => {
                // Block-level elements use their own content-box
                inner_size.width
            }
        };

        LogicalSize {
            width: available_width,
            // Use containing block height!
            height: containing_block_size.height,
        }
    } else {
        // Height is explicit - use inner size (after padding/border)
        node.box_props.inner_size(final_used_size, writing_mode)
    };

    // Proactively reserve space for scrollbars based on overflow properties.
    // If overflow-y is auto/scroll, we must reduce available width for children
    // to ensure they don't overlap with the scrollbar.
    // This is done BEFORE layout so children are sized correctly from the start.
    let scrollbar_reservation = match dom_id {
        Some(id) => {
            let styled_node_state = ctx
                .styled_dom
                .styled_nodes
                .as_container()
                .get(id)
                .map(|s| s.styled_node_state.clone())
                .unwrap_or_default();
            let overflow_y = get_overflow_y(ctx.styled_dom, id, &styled_node_state);
            use azul_css::props::layout::LayoutOverflow;
            match overflow_y.unwrap_or_default() {
                LayoutOverflow::Scroll | LayoutOverflow::Auto => fc::SCROLLBAR_WIDTH_PX,
                _ => 0.0,
            }
        }
        None => 0.0,
    };

    // Reduce available width by scrollbar reservation (if any)
    let available_size_for_children = if scrollbar_reservation > 0.0 {
        LogicalSize {
            width: (available_size_for_children.width - scrollbar_reservation).max(0.0),
            height: available_size_for_children.height,
        }
    } else {
        available_size_for_children
    };

    let constraints = LayoutConstraints {
        available_size: available_size_for_children,
        bfc_state: None,
        writing_mode,
        text_align: style_text_align_to_fc(text_align),
        containing_block_size,
        available_width_type: Text3AvailableSpace::Definite(available_size_for_children.width),
    };

    Ok(PreparedLayoutContext {
        constraints,
        dom_id,
        writing_mode,
        final_used_size,
        box_props: node.box_props.clone(),
    })
}

/// Determines scrollbar requirements for a node based on content overflow.
///
/// Checks if scrollbars are needed by comparing content size against available space.
/// For paged media (PDF), scrollbars are never added since they don't exist in print.
/// Returns the computed ScrollbarRequirements with horizontal/vertical needs and dimensions.
fn compute_scrollbar_info<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    dom_id: NodeId,
    styled_node_state: &azul_core::styled_dom::StyledNodeState,
    content_size: LogicalSize,
    box_props: &crate::solver3::geometry::BoxProps,
    final_used_size: LogicalSize,
    writing_mode: LayoutWritingMode,
) -> ScrollbarRequirements {
    // Skip scrollbar handling for paged media (PDF)
    if ctx.fragmentation_context.is_some() {
        return ScrollbarRequirements {
            needs_horizontal: false,
            needs_vertical: false,
            scrollbar_width: 0.0,
            scrollbar_height: 0.0,
        };
    }

    let overflow_x = get_overflow_x(ctx.styled_dom, dom_id, styled_node_state);
    let overflow_y = get_overflow_y(ctx.styled_dom, dom_id, styled_node_state);

    let container_size = box_props.inner_size(final_used_size, writing_mode);

    fc::check_scrollbar_necessity(
        content_size,
        container_size,
        to_overflow_behavior(overflow_x),
        to_overflow_behavior(overflow_y),
    )
}

/// Checks if scrollbars changed compared to previous layout and if reflow is needed.
///
/// To prevent oscillation, we only trigger reflow when scrollbars are *added*,
/// never when they would be *removed*. This is because:
/// 1. Adding scrollbars reduces available space → content reflows → may fit
/// 2. Removing scrollbars increases space → content reflows → may overflow again
/// This creates an infinite loop. By only allowing transitions *to* scrollbars,
/// we reach a stable state where scrollbars are present if ever needed.
fn check_scrollbar_change(
    tree: &LayoutTree,
    node_index: usize,
    scrollbar_info: &ScrollbarRequirements,
    skip_scrollbar_check: bool,
) -> bool {
    if skip_scrollbar_check {
        return false;
    }

    let Some(current_node) = tree.get(node_index) else {
        return false;
    };

    match &current_node.scrollbar_info {
        None => scrollbar_info.needs_reflow(),
        Some(old_info) => {
            // Only trigger reflow if scrollbars are being ADDED, not removed
            let adding_horizontal = !old_info.needs_horizontal && scrollbar_info.needs_horizontal;
            let adding_vertical = !old_info.needs_vertical && scrollbar_info.needs_vertical;
            adding_horizontal || adding_vertical
        }
    }
}

/// Merges new scrollbar info with existing info, keeping scrollbars once needed.
///
/// This prevents the oscillation problem where content reflows to fit without
/// scrollbars, but then overflows again when scrollbars are removed.
fn merge_scrollbar_info(
    tree: &LayoutTree,
    node_index: usize,
    new_info: &ScrollbarRequirements,
) -> ScrollbarRequirements {
    let Some(current_node) = tree.get(node_index) else {
        return new_info.clone();
    };

    match &current_node.scrollbar_info {
        Some(old) => ScrollbarRequirements {
            needs_horizontal: old.needs_horizontal || new_info.needs_horizontal,
            needs_vertical: old.needs_vertical || new_info.needs_vertical,
            scrollbar_width: if old.needs_vertical || new_info.needs_vertical {
                16.0
            } else {
                0.0
            },
            scrollbar_height: if old.needs_horizontal || new_info.needs_horizontal {
                16.0
            } else {
                0.0
            },
        },
        None => new_info.clone(),
    }
}

/// Calculates the content-box position from a margin-box position.
///
/// The content-box is offset from the margin-box by border + padding.
/// Margin is NOT added here because containing_block_pos already accounts for it.
fn calculate_content_box_pos(
    containing_block_pos: LogicalPosition,
    box_props: &crate::solver3::geometry::BoxProps,
) -> LogicalPosition {
    LogicalPosition::new(
        containing_block_pos.x + box_props.border.left + box_props.padding.left,
        containing_block_pos.y + box_props.border.top + box_props.padding.top,
    )
}

/// Emits debug logging for content-box calculation if debug messages are enabled.
fn log_content_box_calculation<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    node_index: usize,
    current_node: &LayoutNode,
    containing_block_pos: LogicalPosition,
    self_content_box_pos: LogicalPosition,
) {
    let Some(debug_msgs) = ctx.debug_messages.as_mut() else {
        return;
    };

    let dom_name = current_node
        .dom_node_id
        .and_then(|id| {
            ctx.styled_dom
                .node_data
                .as_container()
                .internal
                .get(id.index())
        })
        .map(|n| format!("{:?}", n.node_type))
        .unwrap_or_else(|| "Unknown".to_string());

    debug_msgs.push(LayoutDebugMessage::new(
        LayoutDebugMessageType::PositionCalculation,
        format!(
            "[CONTENT BOX {}] {} - margin-box pos=({:.2}, {:.2}) + border=({:.2},{:.2}) + \
             padding=({:.2},{:.2}) = content-box pos=({:.2}, {:.2})",
            node_index,
            dom_name,
            containing_block_pos.x,
            containing_block_pos.y,
            current_node.box_props.border.left,
            current_node.box_props.border.top,
            current_node.box_props.padding.left,
            current_node.box_props.padding.top,
            self_content_box_pos.x,
            self_content_box_pos.y
        ),
    ));
}

/// Emits debug logging for child positioning if debug messages are enabled.
fn log_child_positioning<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    child_index: usize,
    child_node: &LayoutNode,
    self_content_box_pos: LogicalPosition,
    child_relative_pos: LogicalPosition,
    child_absolute_pos: LogicalPosition,
) {
    let Some(debug_msgs) = ctx.debug_messages.as_mut() else {
        return;
    };

    let child_dom_name = child_node
        .dom_node_id
        .and_then(|id| {
            ctx.styled_dom
                .node_data
                .as_container()
                .internal
                .get(id.index())
        })
        .map(|n| format!("{:?}", n.node_type))
        .unwrap_or_else(|| "Unknown".to_string());

    debug_msgs.push(LayoutDebugMessage::new(
        LayoutDebugMessageType::PositionCalculation,
        format!(
            "[CHILD POS {}] {} - parent content-box=({:.2}, {:.2}) + relative=({:.2}, {:.2}) + \
             margin=({:.2}, {:.2}) = absolute=({:.2}, {:.2})",
            child_index,
            child_dom_name,
            self_content_box_pos.x,
            self_content_box_pos.y,
            child_relative_pos.x,
            child_relative_pos.y,
            child_node.box_props.margin.left,
            child_node.box_props.margin.top,
            child_absolute_pos.x,
            child_absolute_pos.y
        ),
    ));
}

/// Processes a single in-flow child: sets position and recurses.
///
/// For Flex/Grid containers, Taffy has already laid out the children completely.
/// We only recurse to position their grandchildren.
/// For other formatting contexts (Block, Inline, Table), we do full recursive layout.
fn process_inflow_child<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    child_index: usize,
    child_relative_pos: LogicalPosition,
    self_content_box_pos: LogicalPosition,
    inner_size_after_scrollbars: LogicalSize,
    writing_mode: LayoutWritingMode,
    is_flex_or_grid: bool,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
    reflow_needed_for_scrollbars: &mut bool,
    float_cache: &mut BTreeMap<usize, fc::FloatingContext>,
) -> Result<()> {
    // Set relative position on child
    let child_node = tree.get_mut(child_index).ok_or(LayoutError::InvalidTree)?;
    child_node.relative_position = Some(child_relative_pos);

    // Calculate absolute position
    let child_absolute_pos = LogicalPosition::new(
        self_content_box_pos.x + child_relative_pos.x,
        self_content_box_pos.y + child_relative_pos.y,
    );

    // Debug logging
    {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        log_child_positioning(
            ctx,
            child_index,
            child_node,
            self_content_box_pos,
            child_relative_pos,
            child_absolute_pos,
        );
    }

    calculated_positions.insert(child_index, child_absolute_pos);

    // Recurse based on parent's formatting context
    if is_flex_or_grid {
        // For Flex/Grid: Taffy already set used_size. Only recurse for grandchildren.
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_content_box_pos =
            calculate_content_box_pos(child_absolute_pos, &child_node.box_props);
        let child_inner_size = child_node
            .box_props
            .inner_size(child_node.used_size.unwrap_or_default(), writing_mode);

        position_flex_child_descendants(
            ctx,
            tree,
            text_cache,
            child_index,
            child_content_box_pos,
            child_inner_size,
            calculated_positions,
            reflow_needed_for_scrollbars,
            float_cache,
        )?;
    } else {
        // For Block/Inline/Table: do full recursive layout
        calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            child_index,
            child_absolute_pos,
            inner_size_after_scrollbars,
            calculated_positions,
            reflow_needed_for_scrollbars,
            float_cache,
        )?;
    }

    Ok(())
}

/// Processes out-of-flow children (absolute/fixed positioned elements).
///
/// Out-of-flow elements don't appear in layout_output.positions but still need
/// a static position for when no explicit offsets are specified. This sets their
/// static position to the parent's content-box origin.
fn process_out_of_flow_children<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    self_content_box_pos: LogicalPosition,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
) -> Result<()> {
    // Collect out-of-flow children (those not already positioned)
    let out_of_flow_children: Vec<(usize, Option<NodeId>)> = {
        let current_node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        current_node
            .children
            .iter()
            .filter_map(|&child_index| {
                if calculated_positions.contains_key(&child_index) {
                    return None;
                }
                let child = tree.get(child_index)?;
                Some((child_index, child.dom_node_id))
            })
            .collect()
    };

    for (child_index, child_dom_id_opt) in out_of_flow_children {
        let Some(child_dom_id) = child_dom_id_opt else {
            continue;
        };

        let position_type = get_position_type(ctx.styled_dom, Some(child_dom_id));
        if position_type != LayoutPosition::Absolute && position_type != LayoutPosition::Fixed {
            continue;
        }

        // Set static position to parent's content-box origin
        calculated_positions.insert(child_index, self_content_box_pos);

        // Recursively set static positions for nested out-of-flow descendants
        set_static_positions_recursive(
            ctx,
            tree,
            text_cache,
            child_index,
            self_content_box_pos,
            calculated_positions,
        )?;
    }

    Ok(())
}

/// Recursive, top-down pass to calculate used sizes and positions for a given subtree.
/// This is the single, authoritative function for in-flow layout.
pub fn calculate_layout_for_subtree<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    containing_block_pos: LogicalPosition,
    containing_block_size: LogicalSize,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
    reflow_needed_for_scrollbars: &mut bool,
    float_cache: &mut BTreeMap<usize, fc::FloatingContext>,
) -> Result<()> {
    // Phase 1: Prepare layout context (calculate used size, constraints)
    let PreparedLayoutContext {
        constraints,
        dom_id,
        writing_mode,
        mut final_used_size,
        box_props,
    } = {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        prepare_layout_context(ctx, node, containing_block_size)?
    };

    // Phase 2: Layout children using the formatting context
    let layout_result =
        layout_formatting_context(ctx, tree, text_cache, node_index, &constraints, float_cache)?;
    let content_size = layout_result.output.overflow_size;

    // Phase 2.5: Resolve 'auto' main-axis size based on content
    // For anonymous boxes, use default styled node state
    let styled_node_state = dom_id
        .and_then(|id| ctx.styled_dom.styled_nodes.as_container().get(id).cloned())
        .map(|n| n.styled_node_state)
        .unwrap_or_default();

    let css_height: MultiValue<LayoutHeight> = match dom_id {
        Some(id) => get_css_height(ctx.styled_dom, id, &styled_node_state),
        None => MultiValue::Auto, // Anonymous boxes have auto height
    };
    if should_use_content_height(&css_height) {
        final_used_size = apply_content_based_height(
            final_used_size,
            content_size,
            tree,
            node_index,
            writing_mode,
        );
    }

    // Phase 3: Scrollbar handling
    // Anonymous boxes don't have scrollbars
    let skip_scrollbar_check = ctx.fragmentation_context.is_some();
    let scrollbar_info = match dom_id {
        Some(id) => compute_scrollbar_info(
            ctx,
            id,
            &styled_node_state,
            content_size,
            &box_props,
            final_used_size,
            writing_mode,
        ),
        None => ScrollbarRequirements::default(),
    };

    if check_scrollbar_change(tree, node_index, &scrollbar_info, skip_scrollbar_check) {
        *reflow_needed_for_scrollbars = true;
    }

    let merged_scrollbar_info = merge_scrollbar_info(tree, node_index, &scrollbar_info);
    let content_box_size = box_props.inner_size(final_used_size, writing_mode);
    let inner_size_after_scrollbars = merged_scrollbar_info.shrink_size(content_box_size);

    // Phase 4: Update this node's state
    let self_content_box_pos = {
        let current_node = tree.get_mut(node_index).ok_or(LayoutError::InvalidTree)?;

        // Table cells get their size from the table layout algorithm, don't overwrite
        let is_table_cell = matches!(
            current_node.formatting_context,
            FormattingContext::TableCell
        );
        if !is_table_cell || current_node.used_size.is_none() {
            current_node.used_size = Some(final_used_size);
        }
        current_node.scrollbar_info = Some(merged_scrollbar_info);
        // Store overflow content size for scroll frame calculation
        current_node.overflow_content_size = Some(content_size);

        let pos = calculate_content_box_pos(containing_block_pos, &current_node.box_props);
        log_content_box_calculation(ctx, node_index, current_node, containing_block_pos, pos);
        pos
    };

    // Phase 5: Determine formatting context type
    let is_flex_or_grid = {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        matches!(
            node.formatting_context,
            FormattingContext::Flex | FormattingContext::Grid
        )
    };

    // Phase 6: Process in-flow children
    let positions: Vec<_> = layout_result
        .output
        .positions
        .iter()
        .map(|(&idx, &pos)| (idx, pos))
        .collect();

    for (child_index, child_relative_pos) in positions {
        process_inflow_child(
            ctx,
            tree,
            text_cache,
            child_index,
            child_relative_pos,
            self_content_box_pos,
            inner_size_after_scrollbars,
            writing_mode,
            is_flex_or_grid,
            calculated_positions,
            reflow_needed_for_scrollbars,
            float_cache,
        )?;
    }

    // Phase 7: Process out-of-flow children (absolute/fixed)
    process_out_of_flow_children(
        ctx,
        tree,
        text_cache,
        node_index,
        self_content_box_pos,
        calculated_positions,
    )?;

    Ok(())
}

/// Recursively set static positions for out-of-flow descendants without doing layout
/// Recursively positions descendants of Flex/Grid children.
///
/// When a Flex container lays out its children via Taffy, the children have their
/// used_size and relative_position set, but their GRANDCHILDREN don't have positions
/// in calculated_positions yet. This function traverses down the tree and positions
/// all descendants properly.
fn position_flex_child_descendants<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    content_box_pos: LogicalPosition,
    available_size: LogicalSize,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
    reflow_needed_for_scrollbars: &mut bool,
    float_cache: &mut BTreeMap<usize, fc::FloatingContext>,
) -> Result<()> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
    let children: Vec<usize> = node.children.clone();
    let fc = node.formatting_context.clone();

    // If this node is itself a Flex/Grid container, its children were laid out by Taffy
    // and already have relative_position set. We just need to convert to absolute and recurse.
    if matches!(fc, FormattingContext::Flex | FormattingContext::Grid) {
        for &child_index in &children {
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let child_rel_pos = child_node.relative_position.unwrap_or_default();
            let child_abs_pos = LogicalPosition::new(
                content_box_pos.x + child_rel_pos.x,
                content_box_pos.y + child_rel_pos.y,
            );
            
            println!("[POSITION FLEX CHILD] child_index={} relative_pos=({}, {}) content_box_pos=({}, {}) => absolute_pos=({}, {})",
                child_index,
                child_rel_pos.x, child_rel_pos.y,
                content_box_pos.x, content_box_pos.y,
                child_abs_pos.x, child_abs_pos.y
            );

            // Insert position
            calculated_positions.insert(child_index, child_abs_pos);

            // Get child's content box for recursion
            let child_content_box = LogicalPosition::new(
                child_abs_pos.x
                    + child_node.box_props.border.left
                    + child_node.box_props.padding.left,
                child_abs_pos.y
                    + child_node.box_props.border.top
                    + child_node.box_props.padding.top,
            );
            let child_inner_size = child_node.box_props.inner_size(
                child_node.used_size.unwrap_or_default(),
                LayoutWritingMode::HorizontalTb,
            );

            // Recurse
            position_flex_child_descendants(
                ctx,
                tree,
                text_cache,
                child_index,
                child_content_box,
                child_inner_size,
                calculated_positions,
                reflow_needed_for_scrollbars,
                float_cache,
            )?;
        }
    } else {
        // For Block/Inline/Table children, their descendants need proper layout calculation
        // Use the output.positions from their own layout
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        let children: Vec<usize> = node.children.clone();

        for &child_index in &children {
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let child_rel_pos = child_node.relative_position.unwrap_or_default();
            let child_abs_pos = LogicalPosition::new(
                content_box_pos.x + child_rel_pos.x,
                content_box_pos.y + child_rel_pos.y,
            );

            // Insert position
            calculated_positions.insert(child_index, child_abs_pos);

            // Get child's content box for recursion
            let child_content_box = LogicalPosition::new(
                child_abs_pos.x
                    + child_node.box_props.border.left
                    + child_node.box_props.padding.left,
                child_abs_pos.y
                    + child_node.box_props.border.top
                    + child_node.box_props.padding.top,
            );
            let child_inner_size = child_node.box_props.inner_size(
                child_node.used_size.unwrap_or_default(),
                LayoutWritingMode::HorizontalTb,
            );

            // Recurse
            position_flex_child_descendants(
                ctx,
                tree,
                text_cache,
                child_index,
                child_content_box,
                child_inner_size,
                calculated_positions,
                reflow_needed_for_scrollbars,
                float_cache,
            )?;
        }
    }

    Ok(())
}

fn set_static_positions_recursive<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    _text_cache: &mut TextLayoutCache,
    node_index: usize,
    parent_content_box_pos: LogicalPosition,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
) -> Result<()> {
    let out_of_flow_children: Vec<(usize, Option<NodeId>)> = {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        node.children
            .iter()
            .filter_map(|&child_index| {
                if calculated_positions.contains_key(&child_index) {
                    None
                } else {
                    let child = tree.get(child_index)?;
                    Some((child_index, child.dom_node_id))
                }
            })
            .collect()
    };

    for (child_index, child_dom_id_opt) in out_of_flow_children {
        if let Some(child_dom_id) = child_dom_id_opt {
            let position_type = get_position_type(ctx.styled_dom, Some(child_dom_id));
            if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
                calculated_positions.insert(child_index, parent_content_box_pos);

                // Continue recursively
                set_static_positions_recursive(
                    ctx,
                    tree,
                    _text_cache,
                    child_index,
                    parent_content_box_pos,
                    calculated_positions,
                )?;
            }
        }
    }

    Ok(())
}

/// Checks if the given CSS height value should use content-based sizing
fn should_use_content_height(css_height: &MultiValue<LayoutHeight>) -> bool {
    match css_height {
        MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
            // Auto/Initial/Inherit height should use content-based sizing
            true
        }
        MultiValue::Exact(height) => match height {
            LayoutHeight::Auto => {
                // Auto height should use content-based sizing
                true
            }
            LayoutHeight::Px(px) => {
                // Check if it's zero or if it has explicit value
                // If it's a percentage or em, it's not auto
                use azul_css::props::basic::{pixel::PixelValue, SizeMetric};
                px == &PixelValue::zero()
                    || (px.metric != SizeMetric::Px
                        && px.metric != SizeMetric::Percent
                        && px.metric != SizeMetric::Em
                        && px.metric != SizeMetric::Rem)
            }
            LayoutHeight::MinContent | LayoutHeight::MaxContent => {
                // These are content-based, so they should use the content size
                true
            }
        },
    }
}

/// Applies content-based height sizing to a node
///
/// **Note**: This function respects min-height/max-height constraints from Phase 1.
///
/// According to CSS 2.2 § 10.7, when height is 'auto', the final height must be
/// max(min_height, min(content_height, max_height)).
///
/// The `used_size` parameter already contains the size constrained by
/// min-height/max-height from the initial sizing pass. We must take the
/// maximum of this constrained size and the new content-based size to ensure
/// min-height is not lost.
fn apply_content_based_height(
    mut used_size: LogicalSize,
    content_size: LogicalSize,
    tree: &LayoutTree,
    node_index: usize,
    writing_mode: LayoutWritingMode,
) -> LogicalSize {
    let node_props = &tree.get(node_index).unwrap().box_props;
    let main_axis_padding_border =
        node_props.padding.main_sum(writing_mode) + node_props.border.main_sum(writing_mode);

    // CRITICAL: 'old_main_size' holds the size constrained by min-height/max-height from Phase 1
    let old_main_size = used_size.main(writing_mode);
    let new_main_size = content_size.main(writing_mode) + main_axis_padding_border;

    // Final size = max(min_height_constrained_size, content_size)
    // This ensures that min-height is respected even when content is smaller
    let final_main_size = old_main_size.max(new_main_size);

    used_size = used_size.with_main(writing_mode, final_main_size);

    used_size
}

fn hash_styled_node_data(dom: &StyledDom, node_id: NodeId) -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Some(styled_node) = dom.styled_nodes.as_container().get(node_id) {
        styled_node.styled_node_state.hash(&mut hasher);
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

/// Computes CSS counter values for all nodes in the layout tree.
///
/// This function traverses the tree in document order and processes counter-reset
/// and counter-increment properties. The computed values are stored in cache.counters.
///
/// CSS counters work with a stack-based scoping model:
/// - `counter-reset` creates a new scope and sets the counter to a value
/// - `counter-increment` increments the counter in the current scope
/// - When leaving a subtree, counter scopes are popped
pub fn compute_counters(
    styled_dom: &StyledDom,
    tree: &LayoutTree,
    counters: &mut BTreeMap<(usize, String), i32>,
) {
    use std::collections::HashMap;

    // Track counter stacks: counter_name -> Vec<value>
    // Each entry in the Vec represents a nested scope
    let mut counter_stacks: HashMap<String, Vec<i32>> = HashMap::new();

    // Stack to track which counters were reset at each tree level
    // When we pop back up the tree, we need to pop these counter scopes
    let mut scope_stack: Vec<Vec<String>> = Vec::new();

    compute_counters_recursive(
        styled_dom,
        tree,
        tree.root,
        counters,
        &mut counter_stacks,
        &mut scope_stack,
    );
}

fn compute_counters_recursive(
    styled_dom: &StyledDom,
    tree: &LayoutTree,
    node_idx: usize,
    counters: &mut BTreeMap<(usize, String), i32>,
    counter_stacks: &mut std::collections::HashMap<String, Vec<i32>>,
    scope_stack: &mut Vec<Vec<String>>,
) {
    let node = match tree.get(node_idx) {
        Some(n) => n,
        None => return,
    };

    // Skip pseudo-elements (::marker, ::before, ::after) for counter processing
    // Pseudo-elements inherit counter values from their parent element
    // but don't participate in counter-reset or counter-increment themselves
    if node.pseudo_element.is_some() {
        // Store the parent's counter values for this pseudo-element
        // so it can be looked up during marker text generation
        if let Some(parent_idx) = node.parent {
            // Copy all counter values from parent to this pseudo-element
            let parent_counters: Vec<_> = counters
                .iter()
                .filter(|((idx, _), _)| *idx == parent_idx)
                .map(|((_, name), &value)| (name.clone(), value))
                .collect();

            for (counter_name, value) in parent_counters {
                counters.insert((node_idx, counter_name), value);
            }
        }

        // Don't recurse to children of pseudo-elements
        // (pseudo-elements shouldn't have children in normal circumstances)
        return;
    }

    // Only process real DOM nodes, not anonymous boxes
    let dom_id = match node.dom_node_id {
        Some(id) => id,
        None => {
            // For anonymous boxes, just recurse to children
            for &child_idx in &node.children {
                compute_counters_recursive(
                    styled_dom,
                    tree,
                    child_idx,
                    counters,
                    counter_stacks,
                    scope_stack,
                );
            }
            return;
        }
    };

    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let cache = &styled_dom.css_property_cache.ptr;

    // Track which counters we reset at this level (for cleanup later)
    let mut reset_counters_at_this_level = Vec::new();

    // CSS Lists §3: display: list-item automatically increments the "list-item" counter
    // Check if this is a list-item
    let display = cache
        .get_display(node_data, &dom_id, node_state)
        .and_then(|d| d.get_property().copied());
    let is_list_item = matches!(display, Some(LayoutDisplay::ListItem));

    // Process counter-reset (now properly typed)
    let counter_reset = cache
        .get_counter_reset(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property());

    if let Some(counter_reset) = counter_reset {
        let counter_name_str = counter_reset.counter_name.as_str();
        if counter_name_str != "none" {
            let counter_name = counter_name_str.to_string();
            let reset_value = counter_reset.value;

            // Reset the counter by pushing a new scope
            counter_stacks
                .entry(counter_name.clone())
                .or_default()
                .push(reset_value);
            reset_counters_at_this_level.push(counter_name);
        }
    }

    // Process counter-increment (now properly typed)
    let counter_inc = cache
        .get_counter_increment(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property());

    if let Some(counter_inc) = counter_inc {
        let counter_name_str = counter_inc.counter_name.as_str();
        if counter_name_str != "none" {
            let counter_name = counter_name_str.to_string();
            let inc_value = counter_inc.value;

            // Increment the counter in the current scope
            let stack = counter_stacks.entry(counter_name.clone()).or_default();
            if stack.is_empty() {
                // Auto-initialize if counter doesn't exist
                stack.push(inc_value);
            } else if let Some(current) = stack.last_mut() {
                *current += inc_value;
            }
        }
    }

    // CSS Lists §3: display: list-item automatically increments "list-item" counter
    if is_list_item {
        let counter_name = "list-item".to_string();
        let stack = counter_stacks.entry(counter_name.clone()).or_default();
        if stack.is_empty() {
            // Auto-initialize if counter doesn't exist
            stack.push(1);
        } else {
            if let Some(current) = stack.last_mut() {
                *current += 1;
            }
        }
    }

    // Store the current counter values for this node
    for (counter_name, stack) in counter_stacks.iter() {
        if let Some(&value) = stack.last() {
            counters.insert((node_idx, counter_name.clone()), value);
        }
    }

    // Push scope tracking for cleanup
    scope_stack.push(reset_counters_at_this_level.clone());

    // Recurse to children
    for &child_idx in &node.children {
        compute_counters_recursive(
            styled_dom,
            tree,
            child_idx,
            counters,
            counter_stacks,
            scope_stack,
        );
    }

    // Pop counter scopes that were created at this level
    if let Some(reset_counters) = scope_stack.pop() {
        for counter_name in reset_counters {
            if let Some(stack) = counter_stacks.get_mut(&counter_name) {
                stack.pop();
            }
        }
    }
}
