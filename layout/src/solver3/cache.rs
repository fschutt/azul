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
    dom::{FormattingContext, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    styled_dom::{StyledDom, StyledNode},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        layout::{
            LayoutFlexWrap, LayoutJustifyContent, LayoutOverflow, LayoutWrap, LayoutWritingMode,
        },
        property::{CssProperty, CssPropertyType},
        style::StyleTextAlign,
    },
    LayoutDebugMessage, LayoutDebugMessageType,
};

use crate::{
    solver3::{
        fc::{self, layout_formatting_context, LayoutConstraints, OverflowBehavior},
        geometry::PositionedRectangle,
        getters::{
            get_css_height, get_justify_content, get_overflow_x, get_overflow_y, get_text_align,
            get_wrap, get_writing_mode, MultiValue,
        },
        layout_tree::{LayoutNode, LayoutTreeBuilder, SubtreeHash},
        LayoutContext, LayoutError, LayoutTree, Result,
    },
    text3::{
        self,
        cache::{FontLoaderTrait, ParsedFontTrait},
    },
};

/// Convert LayoutOverflow to OverflowBehavior
fn to_overflow_behavior(overflow: MultiValue<LayoutOverflow>) -> fc::OverflowBehavior {
    match overflow.unwrap_or_default() {
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
    pub calculated_positions: BTreeMap<usize, LogicalPosition>,
    /// The viewport size from the last layout pass, used to detect resizes.
    pub viewport: Option<LogicalRect>,
    /// Stable scroll IDs computed from node_data_hash (layout index -> scroll ID)
    pub scroll_ids: BTreeMap<usize, u64>,
    /// Mapping from scroll ID to DOM NodeId for hit testing
    pub scroll_id_to_node_id: BTreeMap<u64, NodeId>,
    /// CSS counter values for each node and counter name.
    /// Key: (layout_index, counter_name), Value: counter value
    /// This stores the computed counter values after processing counter-reset and counter-increment.
    pub counters: BTreeMap<(usize, String), i32>,
    /// Cache of positioned floats for each BFC node (layout_index -> FloatingContext).
    /// This persists float positions across multiple layout passes, ensuring IFC children
    /// always have access to correct float exclusions even when layout is recalculated.
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
pub fn reposition_clean_subtrees<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    tree: &LayoutTree<T>,
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

    if wrap.unwrap_or_default() != LayoutFlexWrap::NoWrap {
        return false;
    }

    // Must be start-aligned, so there's no space distribution to recalculate.
    let justify = get_justify_content(styled_dom, id, &styled_node.state);

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

/// Repositions clean children within a simple block-flow layout (like a BFC or a table-row-group).
/// It stacks children along the main axis, preserving their previously calculated cross-axis
/// alignment.
fn reposition_block_flow_siblings<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    parent_idx: usize,
    parent_node: &LayoutNode<T>,
    tree: &LayoutTree<T>,
    layout_roots: &BTreeSet<usize>,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
) {
    let dom_id = parent_node.dom_node_id.unwrap_or(NodeId::ZERO);
    let styled_node_state = styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| n.state.clone())
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
        let margin_box_main_size =
            child_size.main(writing_mode) + child_node.box_props.margin.main_sum(writing_mode);

        if layout_roots.contains(&child_idx) {
            // This child was DIRTY and has been correctly repositioned.
            // Update the pen to the position immediately after this child.
            let new_pos = match calculated_positions.get(&child_idx) {
                Some(p) => *p,
                None => continue,
            };

            let main_axis_offset = match writing_mode {
                LayoutWritingMode::HorizontalTb => new_pos.y - content_box_origin.y,
                LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => {
                    new_pos.x - content_box_origin.x
                }
            };

            main_pen = main_axis_offset
                + child_size.main(writing_mode)
                + child_node.box_props.margin.main_end(writing_mode);
        } else {
            // This child is CLEAN. Calculate its new position and shift its entire subtree.
            let old_pos = match calculated_positions.get(&child_idx) {
                Some(p) => *p,
                None => continue,
            };

            let new_main_pos = main_pen + child_node.box_props.margin.main_start(writing_mode);
            let old_relative_pos = child_node.relative_position.unwrap_or_default();
            let cross_pos = match writing_mode {
                LayoutWritingMode::HorizontalTb => old_relative_pos.x,
                LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => old_relative_pos.y,
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
pub fn shift_subtree_position<T: ParsedFontTrait>(
    node_idx: usize,
    delta: LogicalPosition,
    tree: &LayoutTree<T>,
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
pub fn reconcile_recursive<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    new_dom_id: NodeId,
    old_tree_idx: Option<usize>,
    new_parent_idx: Option<usize>,
    old_tree: Option<&LayoutTree<T>>,
    new_tree_builder: &mut LayoutTreeBuilder<T>,
    recon: &mut ReconciliationResult,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<usize> {
    use azul_core::dom::NodeType;

    let node_data = &styled_dom.node_data.as_container()[new_dom_id];
    eprintln!(
        "DEBUG reconcile_recursive: new_dom_id={:?}, node_type={:?}, old_tree_idx={:?}, \
         new_parent_idx={:?}",
        new_dom_id,
        node_data.get_node_type(),
        old_tree_idx,
        new_parent_idx
    );

    let old_node = old_tree.and_then(|t| old_tree_idx.and_then(|idx| t.get(idx)));
    let new_node_data_hash = hash_styled_node_data(styled_dom, new_dom_id);

    // A node is dirty if it's new, or if its data/style hash has changed.

    let is_dirty = old_node.map_or(true, |n| new_node_data_hash != n.node_data_hash);

    let new_node_idx = if is_dirty {
        eprintln!("DEBUG reconcile_recursive: Node is DIRTY, creating new node");
        new_tree_builder.create_node_from_dom(styled_dom, new_dom_id, new_parent_idx, debug_messages)?
    } else {
        eprintln!("DEBUG reconcile_recursive: Node is CLEAN, cloning from old");
        new_tree_builder.clone_node_from_old(old_node.unwrap(), new_parent_idx)
    };

    eprintln!(
        "DEBUG reconcile_recursive: Created layout node at index {}",
        new_node_idx
    );

    // CRITICAL: For list-items, create a ::marker pseudo-element as the first child
    // This must be done after the node is created but before processing children
    // Per CSS Lists Module Level 3, ::marker is generated as the first child of list-items
    {
        use azul_css::props::layout::LayoutDisplay;
        let node_data = &styled_dom.node_data.as_container()[new_dom_id];
        let node_state = &styled_dom.styled_nodes.as_container()[new_dom_id].state;
        let cache = &styled_dom.css_property_cache.ptr;
        
        if let Some(display_value) = cache.get_display(node_data, &new_dom_id, node_state) {
            if let Some(display) = display_value.get_property() {
                if *display == LayoutDisplay::ListItem {
                    // Create ::marker pseudo-element for this list-item
                    new_tree_builder.create_marker_pseudo_element(styled_dom, new_dom_id, new_node_idx);
                }
            }
        }
    }

    // Reconcile children to check for structural changes and build the new tree structure.
    let hierarchy_container = styled_dom.node_hierarchy.as_container();
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

        // CSS Spec: Text nodes don't generate layout boxes. They are inline content
        // that is collected and laid out by their parent's inline formatting context.
        // Skip creating layout nodes for text, but still hash them for dirty tracking.
        let node_data = &styled_dom.node_data.as_container()[new_child_dom_id];
        if matches!(node_data.get_node_type(), NodeType::Text(_)) {
            // Hash the text node for subtree tracking purposes
            let text_hash = hash_styled_node_data(styled_dom, new_child_dom_id);
            new_child_hashes.push(text_hash);
            // Mark as different if it's a new text node
            let old_child_idx = old_children_indices.get(i).copied();
            if old_tree
                .and_then(|t| old_child_idx.and_then(|idx| t.get(idx)))
                .is_none()
            {
                children_are_different = true;
            }
            continue; // Skip creating layout node for text
        }

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
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
    reflow_needed_for_scrollbars: &mut bool,
    // Cache of positioned floats for each BFC node
    float_cache: &mut BTreeMap<usize, fc::FloatingContext>,
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
        let writing_mode = get_writing_mode(ctx.styled_dom, dom_id, &styled_node_state).unwrap_or_default(); // This should come from the node's style.
        let text_align = get_text_align(ctx.styled_dom, dom_id, &styled_node_state).unwrap_or_default();

        // IMPORTANT: For the available_size that we pass to children, we need to use
        // the containing_block_size if the current node's height is 'auto'.
        // Otherwise, we would pass 0 as available height to children, which breaks
        // table layout and other auto-height containers.
        let css_height = get_css_height(ctx.styled_dom, dom_id, &styled_node_state);
        let available_size_for_children = if should_use_content_height(&css_height) {
            // Height is auto - use containing block size as available size
            let inner_size = node.box_props.inner_size(final_used_size, writing_mode);
            
            // Debug nodes with margins (likely body)
            let has_margin = node.box_props.margin.left > 0.0 || node.box_props.margin.right > 0.0 
                || node.box_props.margin.top > 0.0 || node.box_props.margin.bottom > 0.0;
            if has_margin {
                eprintln!("\n========== NODE {} WITH MARGINS SIZING DEBUG ==========", node_index);
                eprintln!("  containing_block_size: {:?}", containing_block_size);
                eprintln!("  final_used_size (border-box): {:?}", final_used_size);
                eprintln!("  margins: left={}, right={}, top={}, bottom={}", 
                    node.box_props.margin.left,
                    node.box_props.margin.right,
                    node.box_props.margin.top,
                    node.box_props.margin.bottom);
                eprintln!("  padding: left={}, right={}, top={}, bottom={}", 
                    node.box_props.padding.left,
                    node.box_props.padding.right,
                    node.box_props.padding.top,
                    node.box_props.padding.bottom);
                eprintln!("  border: left={}, right={}, top={}, bottom={}", 
                    node.box_props.border.left,
                    node.box_props.border.right,
                    node.box_props.border.top,
                    node.box_props.border.bottom);
                eprintln!("  inner_size (content-box): {:?}", inner_size);
                eprintln!("  available_size_for_children will be: width={}, height={}", 
                    inner_size.width, containing_block_size.height);
                eprintln!("======================================================\n");
            }
            
            LogicalSize {
                width: inner_size.width,
                height: containing_block_size.height, // Use containing block height!
            }
        } else {
            // Height is explicit - use inner size (after padding/border)
            node.box_props.inner_size(final_used_size, writing_mode)
        };

        let constraints = LayoutConstraints {
            available_size: available_size_for_children,
            bfc_state: None,
            writing_mode,
            text_align: match text_align {
                StyleTextAlign::Start | StyleTextAlign::Left => fc::TextAlign::Start,
                StyleTextAlign::End | StyleTextAlign::Right => fc::TextAlign::End,
                StyleTextAlign::Center => fc::TextAlign::Center,
                StyleTextAlign::Justify => fc::TextAlign::Justify,
            },
        };

        eprintln!(
            "DEBUG calculate_layout_for_subtree: node_index={}, final_used_size={:?}, \
             constraints.available_size={:?}, containing_block_size={:?}",
            node_index, final_used_size, constraints.available_size, containing_block_size
        );

        (
            constraints,
            dom_id,
            writing_mode,
            final_used_size,
            node.box_props.clone(),
        )
    };

    let layout_result = layout_formatting_context(ctx, tree, text_cache, node_index, &constraints, float_cache)?;
    let content_size = layout_result.output.overflow_size;
    
    // Note: escaped_top_margin is handled by the PARENT when positioning this node,
    // not by adjusting our own content-box position. The content-box is always at
    // (border+padding) offset from the margin-box, regardless of escaped margins.
    if let Some(escaped_margin) = layout_result.escaped_top_margin {
        eprintln!("[calculate_layout_for_subtree] Node {} has escaped_top_margin={} (will be handled by parent)", node_index, escaped_margin);
    }

    // --- Phase 2.5: Resolve 'auto' main-axis size ---

    // If the node's main-axis size depends on its content, we update its used size now.
    let styled_node_state = ctx
        .styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| n.state.clone())
        .unwrap_or_default();

    let css_height = get_css_height(ctx.styled_dom, dom_id, &styled_node_state);

    if should_use_content_height(&css_height) {
        final_used_size = apply_content_based_height(
            final_used_size,
            content_size,
            tree,
            node_index,
            writing_mode,
        );
    }

    // --- Phase 3: Check for scrollbars and potential reflow ---

    let overflow_x = get_overflow_x(ctx.styled_dom, dom_id, &styled_node_state);
    let overflow_y = get_overflow_y(ctx.styled_dom, dom_id, &styled_node_state);

    let scrollbar_info = fc::check_scrollbar_necessity(
        content_size,
        box_props.inner_size(final_used_size, writing_mode),
        to_overflow_behavior(overflow_x),
        to_overflow_behavior(overflow_y),
    );

    // Check if scrollbar situation changed compared to previous layout
    let scrollbar_changed = {
        let current_node = tree.get(node_index).unwrap();
        match &current_node.scrollbar_info {
            None => scrollbar_info.needs_reflow(), /* First layout, check if scrollbars will */
            // reduce size
            Some(old_info) => {
                // Compare if scrollbar necessity changed
                old_info.needs_horizontal != scrollbar_info.needs_horizontal
                    || old_info.needs_vertical != scrollbar_info.needs_vertical
            }
        }
    };

    if scrollbar_changed {
        *reflow_needed_for_scrollbars = true;
        // Store the new scrollbar info before returning
        let current_node = tree.get_mut(node_index).unwrap();
        current_node.scrollbar_info = Some(scrollbar_info);
        return Ok(());
    }

    let content_box_size = box_props.inner_size(final_used_size, writing_mode);
    let inner_size_after_scrollbars = scrollbar_info.shrink_size(content_box_size);

    // --- Phase 4: Update self and recurse to children ---
    let current_node = tree.get_mut(node_index).unwrap();
    
    // IMPORTANT: Table cells get their size set by the table layout algorithm (position_table_cells).
    // We should NOT overwrite it here, as that would reset the height to 0 (from inline content).
    // Only set used_size if it hasn't been set yet, or if this is not a table cell.
    let is_table_cell = matches!(current_node.formatting_context, FormattingContext::TableCell);
    if !is_table_cell || current_node.used_size.is_none() {
        if is_table_cell {
            println!("[cache] Cell {}: BEFORE overwrite check: used_size={:?}, final_used_size={:?}", 
                node_index, current_node.used_size, final_used_size);
        }
        current_node.used_size = Some(final_used_size);
    } else if is_table_cell {
        println!("[cache] Cell {}: SKIPPED overwrite: keeping used_size={:?}, would have set to {:?}", 
            node_index, current_node.used_size, final_used_size);
    }
    
    current_node.scrollbar_info = Some(scrollbar_info); // Store scrollbar info

    // The absolute position of this node's content-box for its children.
    // containing_block_pos is the absolute position of this node's margin-box.
    // To get to the content-box, we add: border + padding (NOT margin, that's already in containing_block_pos)
    let self_content_box_pos = LogicalPosition::new(
        containing_block_pos.x + current_node.box_props.border.left + current_node.box_props.padding.left,
        containing_block_pos.y + current_node.box_props.border.top + current_node.box_props.padding.top,
    );

    // DEBUG: Log content-box calculation
    if let Some(debug_msgs) = ctx.debug_messages.as_mut() {
        let dom_name = current_node.dom_node_id
            .and_then(|id| ctx.styled_dom.node_data.as_container().internal.get(id.index()))
            .map(|n| format!("{:?}", n.node_type))
            .unwrap_or_else(|| "Unknown".to_string());
        
        debug_msgs.push(LayoutDebugMessage::new(
            LayoutDebugMessageType::PositionCalculation,
            format!("[CONTENT BOX {}] {} - margin-box pos=({:.2}, {:.2}) + border=({:.2},{:.2}) + padding=({:.2},{:.2}) = content-box pos=({:.2}, {:.2})",
                node_index, dom_name, 
                containing_block_pos.x, containing_block_pos.y,
                current_node.box_props.border.left, current_node.box_props.border.top,
                current_node.box_props.padding.left, current_node.box_props.padding.top,
                self_content_box_pos.x, self_content_box_pos.y)
        ));
    }

    // Check if this node is a Flex or Grid container
    let is_flex_or_grid = {
        let node = tree.get(node_index).unwrap();
        matches!(
            node.formatting_context,
            FormattingContext::Flex | FormattingContext::Grid
        )
    };

    // For Block Formatting Context, we need to calculate proper positions AFTER
    // children have been laid out and have their sizes.
    let is_block_fc = {
        let node = tree.get(node_index).unwrap();
        matches!(node.formatting_context, FormattingContext::Block { .. })
    };

    // First pass: recursively layout children so they have used_size
    for (&child_index, &child_relative_pos) in &layout_result.output.positions {
        let child_node = tree.get_mut(child_index).ok_or(LayoutError::InvalidTree)?;
        child_node.relative_position = Some(child_relative_pos);

        let child_absolute_pos = LogicalPosition::new(
            self_content_box_pos.x + child_relative_pos.x,
            self_content_box_pos.y + child_relative_pos.y,
        );
        
        // DEBUG: Log child positioning
        if let Some(debug_msgs) = ctx.debug_messages.as_mut() {
            let child_dom_name = child_node.dom_node_id
                .and_then(|id| ctx.styled_dom.node_data.as_container().internal.get(id.index()))
                .map(|n| format!("{:?}", n.node_type))
                .unwrap_or_else(|| "Unknown".to_string());
            
            debug_msgs.push(LayoutDebugMessage::new(
                LayoutDebugMessageType::PositionCalculation,
                format!("[CHILD POS {}] {} - parent content-box=({:.2}, {:.2}) + relative=({:.2}, {:.2}) + margin=({:.2}, {:.2}) = absolute=({:.2}, {:.2})",
                    child_index, child_dom_name,
                    self_content_box_pos.x, self_content_box_pos.y,
                    child_relative_pos.x, child_relative_pos.y,
                    child_node.box_props.margin.left, child_node.box_props.margin.top,
                    child_absolute_pos.x, child_absolute_pos.y)
            ));
        }
        
        calculated_positions.insert(child_index, child_absolute_pos);

        // For Flex/Grid containers, Taffy has already laid out the children completely
        // (including their used_size and relative_position). We should NOT call
        // calculate_layout_for_subtree recursively because that would overwrite
        // Taffy's calculations with incorrect values.
        //
        // For other formatting contexts (Block, Inline, Table), we need to recurse
        // to lay out the children's subtrees.
        if !is_flex_or_grid {
            // CRITICAL: Pass child_absolute_pos as containing_block_pos for the recursive call.
            // The recursive function interprets containing_block_pos as the absolute position
            // of the current node (which will add padding to get content-box position for its
            // children).
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
    }

    // Second pass: Set static positions for out-of-flow children
    // Out-of-flow elements (absolute/fixed) don't appear in layout_output.positions,
    // but they still need a static position for when no explicit offsets are specified
    let out_of_flow_children: Vec<(usize, Option<NodeId>)> = {
        let current_node = tree.get(node_index).unwrap();
        current_node.children.iter()
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
            let position_type = crate::solver3::positioning::get_position_type(ctx.styled_dom, Some(child_dom_id));
            if position_type == azul_css::props::layout::LayoutPosition::Absolute 
                || position_type == azul_css::props::layout::LayoutPosition::Fixed {
                // Set static position to parent's content-box origin
                // This is where the element would appear if it were in normal flow
                calculated_positions.insert(child_index, self_content_box_pos);
                
                eprintln!("[cache] Set static position for out-of-flow child {} at {:?}", 
                    child_index, self_content_box_pos);
                
                // Recursively set static positions for nested out-of-flow descendants
                // but DON'T do full layout (that happens in position_out_of_flow_elements)
                set_static_positions_recursive(
                    ctx,
                    tree,
                    text_cache,
                    child_index,
                    self_content_box_pos,
                    calculated_positions,
                )?;
            }
        }
    }

    Ok(())
}

/// Recursively set static positions for out-of-flow descendants without doing layout
fn set_static_positions_recursive<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    _text_cache: &mut text3::cache::LayoutCache<T>,
    node_index: usize,
    parent_content_box_pos: LogicalPosition,
    calculated_positions: &mut BTreeMap<usize, LogicalPosition>,
) -> Result<()> {
    let out_of_flow_children: Vec<(usize, Option<NodeId>)> = {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        node.children.iter()
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
            let position_type = crate::solver3::positioning::get_position_type(ctx.styled_dom, Some(child_dom_id));
            if position_type == azul_css::props::layout::LayoutPosition::Absolute 
                || position_type == azul_css::props::layout::LayoutPosition::Fixed {
                calculated_positions.insert(child_index, parent_content_box_pos);
                
                eprintln!("[cache] Set static position for nested out-of-flow child {} at {:?}", 
                    child_index, parent_content_box_pos);
                
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
fn should_use_content_height(css_height: &MultiValue<azul_css::props::layout::LayoutHeight>) -> bool {
    match css_height {
        MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
            // Auto/Initial/Inherit height should use content-based sizing
            true
        }
        MultiValue::Exact(height) => match height {
            azul_css::props::layout::LayoutHeight::Auto => {
                // Auto height should use content-based sizing
                true
            }
            azul_css::props::layout::LayoutHeight::Px(px) => {
                // Check if it's zero or if it has explicit value
                // If it's a percentage or em, it's not auto
                px == &azul_css::props::basic::pixel::PixelValue::zero()
                    || (px.metric != azul_css::props::basic::SizeMetric::Px 
                        && px.metric != azul_css::props::basic::SizeMetric::Percent
                        && px.metric != azul_css::props::basic::SizeMetric::Em
                        && px.metric != azul_css::props::basic::SizeMetric::Rem)
            }
            azul_css::props::layout::LayoutHeight::MinContent
            | azul_css::props::layout::LayoutHeight::MaxContent => {
                // These are content-based, so they should use the content size
                true
            }
        }
    }
}

/// Applies content-based height sizing to a node
/// 
/// CRITICAL FIX: This function now respects min-height/max-height constraints from Phase 1.
/// According to CSS 2.2 § 10.7, when height is 'auto', the final height must be:
///   max(min_height, min(content_height, max_height))
/// 
/// The `used_size` parameter already contains the size constrained by min-height/max-height
/// from the initial sizing pass. We must take the maximum of this constrained size and 
/// the new content-based size to ensure min-height is not lost.
fn apply_content_based_height<T: ParsedFontTrait>(
    mut used_size: LogicalSize,
    content_size: LogicalSize,
    tree: &LayoutTree<T>,
    node_index: usize,
    writing_mode: azul_css::props::layout::LayoutWritingMode,
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

    eprintln!(
        "[apply_content_based_height] node={}: old_main={:.2} (Phase 1 with min-height), \
         content={:.2}, padding+border={:.2}, new_content={:.2}, final={:.2}",
        node_index, old_main_size, content_size.main(writing_mode), 
        main_axis_padding_border, new_main_size, final_main_size
    );

    used_size
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

/// Computes CSS counter values for all nodes in the layout tree.
///
/// This function traverses the tree in document order and processes counter-reset
/// and counter-increment properties. The computed values are stored in cache.counters.
///
/// CSS counters work with a stack-based scoping model:
/// - `counter-reset` creates a new scope and sets the counter to a value
/// - `counter-increment` increments the counter in the current scope
/// - When leaving a subtree, counter scopes are popped
pub fn compute_counters<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    tree: &LayoutTree<T>,
    counters: &mut BTreeMap<(usize, String), i32>,
) {
    use azul_css::props::property::CssProperty;
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

fn compute_counters_recursive<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    tree: &LayoutTree<T>,
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
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].state;
    let cache = &styled_dom.css_property_cache.ptr;
    
    // Track which counters we reset at this level (for cleanup later)
    let mut reset_counters_at_this_level = Vec::new();
    
    // CSS Lists §3: display: list-item automatically increments the "list-item" counter
    // Check if this is a list-item
    let display = cache.get_display(node_data, &dom_id, node_state)
        .and_then(|d| d.get_property().copied());
    let is_list_item = matches!(display, Some(azul_css::props::layout::LayoutDisplay::ListItem));
    
    // Process counter-reset (now properly typed)
    if let Some(counter_reset_value) = cache.get_counter_reset(node_data, &dom_id, node_state) {
        if let Some(counter_reset) = counter_reset_value.get_property() {
            let counter_name_str = counter_reset.counter_name.as_str();
            if counter_name_str != "none" {
                let counter_name = counter_name_str.to_string();
                let reset_value = counter_reset.value;
                
                eprintln!("[compute_counters] Resetting counter '{}' to {} at node_idx={}", counter_name, reset_value, node_idx);
                
                // Reset the counter by pushing a new scope
                counter_stacks
                    .entry(counter_name.clone())
                    .or_default()
                    .push(reset_value);
                reset_counters_at_this_level.push(counter_name);
            }
        }
    }
    
    // Process counter-increment (now properly typed)
    if let Some(counter_inc_value) = cache.get_counter_increment(node_data, &dom_id, node_state) {
        if let Some(counter_inc) = counter_inc_value.get_property() {
            let counter_name_str = counter_inc.counter_name.as_str();
            if counter_name_str != "none" {
                let counter_name = counter_name_str.to_string();
                let inc_value = counter_inc.value;
                
                // Increment the counter in the current scope
                let stack = counter_stacks.entry(counter_name.clone()).or_default();
                if stack.is_empty() {
                    // Auto-initialize if counter doesn't exist
                    stack.push(inc_value);
                } else {
                    if let Some(current) = stack.last_mut() {
                        *current += inc_value;
                    }
                }
            }
        }
    }
    
    // CSS Lists §3: display: list-item automatically increments "list-item" counter
    if is_list_item {
        eprintln!("[compute_counters] ✓ Found list-item at node_idx={}, auto-incrementing counter", node_idx);
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
            eprintln!("[compute_counters] Storing counter '{}' = {} for node_idx={}", counter_name, value, node_idx);
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
