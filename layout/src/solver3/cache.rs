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
    diff::NodeDataFingerprint,
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
            get_overflow_y, get_text_align, get_white_space_property, get_wrap, get_writing_mode,
            MultiValue,
        },
        layout_tree::{
            is_block_level, AnonymousBoxType, DirtyFlag, LayoutNode, LayoutTreeBuilder, SubtreeHash,
        },
        positioning::get_position_type,
        scrollbar::ScrollbarRequirements,
        sizing::calculate_used_size_for_node,
        LayoutContext, LayoutError, LayoutTree, Result,
    },
    text3::cache::AvailableSpace as Text3AvailableSpace,
};

// ============================================================================
// Per-Node Multi-Slot Cache (inspired by Taffy's 9+1 slot cache architecture)
//
// Instead of a global BTreeMap keyed by (node_index, available_size), each node
// gets its own deterministic cache with 9 measurement slots + 1 full layout slot.
// This eliminates O(log n) lookups, prevents slot collisions between MinContent/
// MaxContent/Definite measurements, and cleanly separates sizing from positioning.
//
// Reference: https://github.com/DioxusLabs/taffy — Cache struct in src/tree/cache.rs
// Azul improvement: cache is EXTERNAL (Vec<NodeCache> parallel to LayoutTree.nodes)
// rather than stored on the node, keeping LayoutNode slim and avoiding &mut tree
// for cache operations.
// ============================================================================

/// Determines whether `calculate_layout_for_subtree` should only compute
/// the node's size (for parent's sizing pass) or perform full layout
/// including child positioning.
///
/// Inspired by Taffy's `RunMode` enum. The two-mode approach enables the
/// classic CSS two-pass layout: Pass 1 (ComputeSize) measures all children,
/// Pass 2 (PerformLayout) positions them using the measured sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeMode {
    /// Only compute the node's border-box size and baseline.
    /// Does NOT store child positions. Used in BFC Pass 1 (sizing).
    ComputeSize,
    /// Compute size AND position all children.
    /// Stores the full layout result including child positions.
    /// Used in BFC Pass 2 (positioning) and as the final layout step.
    PerformLayout,
}

/// Constraint classification for deterministic cache slot selection.
///
/// Inspired by Taffy's `AvailableSpace` enum. Each constraint type maps to a
/// different cache slot, preventing collisions between e.g. MinContent and
/// Definite measurements of the same node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailableWidthType {
    /// A definite pixel value (or percentage resolved to pixels).
    Definite,
    /// Shrink-to-fit: the smallest size that doesn't cause overflow.
    MinContent,
    /// Use all available space: the largest size the content can use.
    MaxContent,
}

/// Cache entry for sizing (ComputeSize mode) — stores NO positions.
///
/// This is the lightweight entry stored in the 9 measurement slots.
/// It records what constraints were provided and what size resulted,
/// enabling Taffy's "result matches request" optimization.
#[derive(Debug, Clone)]
pub struct SizingCacheEntry {
    /// The available size that was provided as input.
    pub available_size: LogicalSize,
    /// The computed border-box size (output).
    pub result_size: LogicalSize,
    /// Baseline for inline alignment (if applicable).
    pub baseline: Option<f32>,
    /// First child's escaped top margin (CSS 2.2 § 8.3.1).
    pub escaped_top_margin: Option<f32>,
    /// Last child's escaped bottom margin (CSS 2.2 § 8.3.1).
    pub escaped_bottom_margin: Option<f32>,
}

/// Cache entry for full layout (PerformLayout mode).
///
/// This is the single "final layout" slot. It includes child positions
/// (relative to parent's content-box) and overflow/scrollbar info.
#[derive(Debug, Clone)]
pub struct LayoutCacheEntry {
    /// The available size that was provided as input.
    pub available_size: LogicalSize,
    /// The computed border-box size (output).
    pub result_size: LogicalSize,
    /// Content overflow size (for scrolling).
    pub content_size: LogicalSize,
    /// Child positions relative to parent's content-box (NOT absolute).
    pub child_positions: Vec<(usize, LogicalPosition)>,
    /// First child's escaped top margin.
    pub escaped_top_margin: Option<f32>,
    /// Last child's escaped bottom margin.
    pub escaped_bottom_margin: Option<f32>,
    /// Scrollbar requirements for this node.
    pub scrollbar_info: ScrollbarRequirements,
}

/// Per-node cache entry with 9 measurement slots + 1 full layout slot.
///
/// Inspired by Taffy's `Cache` struct (9+1 slots per node). The deterministic
/// slot index is computed from the constraint combination, so entries never
/// clobber each other (unlike the old global BTreeMap where fixed-point
/// collisions were possible).
///
/// NOT stored on LayoutNode — lives in the external `LayoutCacheMap`.
#[derive(Debug, Clone)]
pub struct NodeCache {
    /// 9 measurement slots (Taffy's deterministic scheme):
    /// - Slot 0: both dimensions known
    /// - Slots 1-2: only width known (MaxContent/Definite vs MinContent)
    /// - Slots 3-4: only height known (MaxContent/Definite vs MinContent)
    /// - Slots 5-8: neither known (2×2 combos of width/height constraint types)
    pub measure_entries: [Option<SizingCacheEntry>; 9],

    /// 1 full layout slot (with child positions, overflow, baseline).
    /// Only populated after PerformLayout, not after ComputeSize.
    pub layout_entry: Option<LayoutCacheEntry>,

    /// Fast check for dirty propagation (Taffy optimization).
    /// When true, all slots are empty — ancestors are also dirty.
    pub is_empty: bool,
}

impl Default for NodeCache {
    fn default() -> Self {
        Self {
            measure_entries: [None, None, None, None, None, None, None, None, None],
            layout_entry: None,
            is_empty: true, // fresh cache is empty/dirty
        }
    }
}

impl NodeCache {
    /// Clear all cache entries, marking this node as dirty.
    pub fn clear(&mut self) {
        self.measure_entries = [None, None, None, None, None, None, None, None, None];
        self.layout_entry = None;
        self.is_empty = true;
    }

    /// Compute the deterministic slot index from constraint dimensions.
    ///
    /// This is Taffy's slot selection scheme: given whether width/height are
    /// "known" (definite constraint provided by parent) and what type of
    /// constraint applies to the unknown dimension(s), we get a unique slot 0–8.
    pub fn slot_index(
        width_known: bool,
        height_known: bool,
        width_type: AvailableWidthType,
        height_type: AvailableWidthType,
    ) -> usize {
        match (width_known, height_known) {
            (true, true) => 0,
            (true, false) => {
                if width_type == AvailableWidthType::MinContent { 2 } else { 1 }
            }
            (false, true) => {
                if height_type == AvailableWidthType::MinContent { 4 } else { 3 }
            }
            (false, false) => {
                let w = if width_type == AvailableWidthType::MinContent { 1 } else { 0 };
                let h = if height_type == AvailableWidthType::MinContent { 1 } else { 0 };
                5 + w * 2 + h
            }
        }
    }

    /// Look up a sizing cache entry, implementing Taffy's "result matches request"
    /// optimization: if the caller provides the result size as a known dimension
    /// (common in Pass1→Pass2 transitions), it's still a cache hit.
    pub fn get_size(&self, slot: usize, known_dims: LogicalSize) -> Option<&SizingCacheEntry> {
        let entry = self.measure_entries[slot].as_ref()?;
        // Exact match on input constraints
        if (known_dims.width - entry.available_size.width).abs() < 0.1
            && (known_dims.height - entry.available_size.height).abs() < 0.1
        {
            return Some(entry);
        }
        // "Result matches request" — if the caller provides the result size
        // as a known dimension, it's still a hit. This is the key optimization
        // that makes two-pass layout O(n): Pass 1 measures a node, Pass 2
        // provides the measured size as a constraint → automatic cache hit.
        if (known_dims.width - entry.result_size.width).abs() < 0.1
            && (known_dims.height - entry.result_size.height).abs() < 0.1
        {
            return Some(entry);
        }
        None
    }

    /// Store a sizing result in the given slot.
    pub fn store_size(&mut self, slot: usize, entry: SizingCacheEntry) {
        self.measure_entries[slot] = Some(entry);
        self.is_empty = false;
    }

    /// Look up the full layout cache entry.
    pub fn get_layout(&self, known_dims: LogicalSize) -> Option<&LayoutCacheEntry> {
        let entry = self.layout_entry.as_ref()?;
        if (known_dims.width - entry.available_size.width).abs() < 0.1
            && (known_dims.height - entry.available_size.height).abs() < 0.1
        {
            return Some(entry);
        }
        // "Result matches request" for layout too
        if (known_dims.width - entry.result_size.width).abs() < 0.1
            && (known_dims.height - entry.result_size.height).abs() < 0.1
        {
            return Some(entry);
        }
        None
    }

    /// Store a full layout result.
    pub fn store_layout(&mut self, entry: LayoutCacheEntry) {
        self.layout_entry = Some(entry);
        self.is_empty = false;
    }
}

/// External layout cache, parallel to `LayoutTree.nodes`.
///
/// `cache_map.entries[i]` holds the cache for `LayoutTree.nodes[i]`.
/// Stored on `LayoutCache` (persists across frames).
///
/// This is Azul's improvement over Taffy's on-node cache:
/// - `LayoutNode` stays slim (0 bytes overhead)
/// - No `&mut tree` needed to read/write cache entries
/// - Cache can be resized independently after reconciliation
/// - O(1) indexed lookup (Vec) instead of O(log n) (BTreeMap)
#[derive(Debug, Clone, Default)]
pub struct LayoutCacheMap {
    pub entries: Vec<NodeCache>,
}

impl LayoutCacheMap {
    /// Resize to match tree length after reconciliation.
    /// New nodes get empty (dirty) caches. Removed nodes' caches are dropped.
    pub fn resize_to_tree(&mut self, tree_len: usize) {
        self.entries.resize_with(tree_len, NodeCache::default);
    }

    /// O(1) lookup by layout tree index.
    #[inline]
    pub fn get(&self, node_index: usize) -> &NodeCache {
        &self.entries[node_index]
    }

    /// O(1) mutable lookup by layout tree index.
    #[inline]
    pub fn get_mut(&mut self, node_index: usize) -> &mut NodeCache {
        &mut self.entries[node_index]
    }

    /// Invalidate a node and propagate dirty flags upward through ancestors.
    ///
    /// Implements Taffy's early-stop optimization: propagation halts at the
    /// first ancestor whose cache is already empty (i.e., already dirty).
    /// This prevents redundant O(depth) propagation when multiple children
    /// of the same parent are dirtied.
    pub fn mark_dirty(&mut self, node_index: usize, tree: &[LayoutNode]) {
        if node_index >= self.entries.len() {
            return;
        }
        let cache = &mut self.entries[node_index];
        if cache.is_empty {
            return; // Already dirty → ancestors are too
        }
        cache.clear();

        // Propagate upward (Taffy's early-stop optimization)
        let mut current = tree.get(node_index).and_then(|n| n.parent);
        while let Some(parent_idx) = current {
            if parent_idx >= self.entries.len() {
                break;
            }
            let parent_cache = &mut self.entries[parent_idx];
            if parent_cache.is_empty {
                break; // Stop early — ancestor already dirty
            }
            parent_cache.clear();
            current = tree.get(parent_idx).and_then(|n| n.parent);
        }
    }
}

/// The persistent cache that holds the layout state between frames.
#[derive(Debug, Clone, Default)]
pub struct LayoutCache {
    /// The fully laid-out tree from the previous frame. This is our primary cache.
    pub tree: Option<LayoutTree>,
    /// The final, absolute positions of all nodes from the previous frame.
    pub calculated_positions: super::PositionVec,
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
    /// Per-node multi-slot cache (inspired by Taffy's 9+1 architecture).
    /// External to LayoutTree — indexed by node index for O(1) lookup.
    /// Persists across frames; resized after reconciliation.
    pub cache_map: LayoutCacheMap,
}

/// The result of a reconciliation pass.
#[derive(Debug, Default)]
pub struct ReconciliationResult {
    /// Set of nodes whose intrinsic size needs to be recalculated (bottom-up pass).
    pub intrinsic_dirty: BTreeSet<usize>,
    /// Set of layout roots whose subtrees need a new top-down layout pass.
    pub layout_roots: BTreeSet<usize>,
    /// Set of nodes that only need a paint/display-list update (no relayout).
    pub paint_dirty: BTreeSet<usize>,
}

impl ReconciliationResult {
    /// Checks if any layout or paint work is needed.
    pub fn is_clean(&self) -> bool {
        self.intrinsic_dirty.is_empty()
            && self.layout_roots.is_empty()
            && self.paint_dirty.is_empty()
    }

    /// Returns true if full layout work is needed for at least one node.
    pub fn needs_layout(&self) -> bool {
        !self.intrinsic_dirty.is_empty() || !self.layout_roots.is_empty()
    }

    /// Returns true if only paint work is needed (no layout).
    pub fn needs_paint_only(&self) -> bool {
        !self.needs_layout() && !self.paint_dirty.is_empty()
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
    calculated_positions: &mut super::PositionVec,
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
/// CSS Overflow Module Level 3: initial value of `overflow` is `visible`.
pub fn to_overflow_behavior(overflow: MultiValue<LayoutOverflow>) -> fc::OverflowBehavior {
    match overflow.unwrap_or(LayoutOverflow::Visible) {
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
    calculated_positions: &mut super::PositionVec,
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
        .get(parent_idx)
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
            let new_pos = match calculated_positions.get(child_idx) {
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
            let old_pos = match calculated_positions.get(child_idx) {
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
    calculated_positions: &mut super::PositionVec,
) {
    if let Some(pos) = calculated_positions.get_mut(node_idx) {
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
    let mut new_tree_builder = LayoutTreeBuilder::new(ctx.viewport_size);
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

/// CSS 2.2 § 9.2.2.1: Checks whether an inline run consists entirely of
/// whitespace-only text nodes, in which case it should NOT generate an
/// anonymous IFC wrapper in a BFC mixed-content context.
///
/// This prevents whitespace between block elements from creating empty
/// anonymous blocks that take up vertical space (regression c33e94b0).
///
/// Exception: if the parent (or any ancestor) has `white-space: pre`,
/// `pre-wrap`, or `pre-line`, whitespace IS significant and the wrapper
/// must still be created.
fn is_whitespace_only_inline_run(
    styled_dom: &StyledDom,
    inline_run: &[(usize, NodeId)],
    parent_dom_id: NodeId,
) -> bool {
    use azul_css::props::style::text::StyleWhiteSpace;

    if inline_run.is_empty() {
        return true;
    }

    // Check if the parent preserves whitespace
    let parent_state = &styled_dom.styled_nodes.as_container()[parent_dom_id].styled_node_state;
    let white_space = match get_white_space_property(styled_dom, parent_dom_id, parent_state) {
        MultiValue::Exact(ws) => Some(ws),
        _ => None,
    };

    // If white-space preserves whitespace, don't strip
    if matches!(
        white_space,
        Some(StyleWhiteSpace::Pre) | Some(StyleWhiteSpace::PreWrap) | Some(StyleWhiteSpace::PreLine)
    ) {
        return false;
    }

    // Check that every node in the run is a whitespace-only text node
    let binding = styled_dom.node_data.as_container();
    for &(_, dom_id) in inline_run {
        if let Some(data) = binding.get(dom_id) {
            match data.get_node_type() {
                NodeType::Text(text) => {
                    let s = text.as_str();
                    if !s.chars().all(|c| c.is_whitespace()) {
                        return false; // Non-whitespace text → must create wrapper
                    }
                }
                _ => {
                    return false; // Non-text inline element → must create wrapper
                }
            }
        }
    }

    true // All nodes are whitespace-only text
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

    // Compute the new multi-field fingerprint instead of a single hash.
    let new_fingerprint = NodeDataFingerprint::compute(
        node_data,
        styled_dom.styled_nodes.as_container().get(new_dom_id).map(|n| &n.styled_node_state),
    );

    // Compare fingerprints to determine what changed (Layout, Paint, or Nothing).
    let dirty_flag = match old_node {
        None => DirtyFlag::Layout, // new node → full layout
        Some(old_n) => {
            let change_set = old_n.node_data_fingerprint.diff(&new_fingerprint);
            if change_set.needs_layout() {
                DirtyFlag::Layout
            } else if change_set.needs_paint() {
                DirtyFlag::Paint
            } else {
                DirtyFlag::None
            }
        }
    };
    let is_dirty = dirty_flag >= DirtyFlag::Paint;

    let new_node_idx = if dirty_flag >= DirtyFlag::Layout {
        new_tree_builder.create_node_from_dom(
            styled_dom,
            new_dom_id,
            new_parent_idx,
            debug_messages,
        )?
    } else {
        // Paint-only or clean: clone the old node (preserving layout cache)
        let mut idx = new_tree_builder.clone_node_from_old(old_node.unwrap(), new_parent_idx);
        // If paint-only change, update the fingerprint and dirty flag
        if dirty_flag == DirtyFlag::Paint {
            if let Some(cloned) = new_tree_builder.get_mut(idx) {
                cloned.node_data_fingerprint = new_fingerprint;
                cloned.dirty_flag = DirtyFlag::Paint;
            }
        }
        idx
    };

    // CRITICAL: For list-items, create a ::marker pseudo-element as the first child
    // This must be done after the node is created but before processing children
    // Per CSS Lists Module Level 3, ::marker is generated as the first child of list-items
    {
        use crate::solver3::getters::get_display_property;
        let display = get_display_property(styled_dom, Some(new_dom_id))
            .exact();

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
                    // CSS 2.2 § 9.2.2.1: If the inline run consists entirely of
                    // whitespace-only text nodes (and white-space doesn't preserve it),
                    // skip creating the anonymous IFC wrapper. This prevents inter-block
                    // whitespace from creating empty blocks that take up vertical space.
                    if is_whitespace_only_inline_run(styled_dom, &inline_run, new_dom_id) {
                        if let Some(msgs) = debug_messages.as_mut() {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[reconcile_recursive] Skipping whitespace-only inline run ({} nodes) between blocks in node {}",
                                inline_run.len(),
                                new_dom_id.index()
                            )));
                        }
                        inline_run.clear();
                    } else {
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
                    } // end else (non-whitespace run)
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
            // CSS 2.2 § 9.2.2.1: Skip whitespace-only trailing inline runs
            if is_whitespace_only_inline_run(styled_dom, &inline_run, new_dom_id) {
                if let Some(msgs) = debug_messages.as_mut() {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[reconcile_recursive] Skipping trailing whitespace-only inline run ({} nodes) in node {}",
                        inline_run.len(),
                        new_dom_id.index()
                    )));
                }
                // Don't create a wrapper — just drop the run
            } else {
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
            } // end else (non-whitespace trailing run)
        }
    }

    // After reconciling children, calculate this node's full subtree hash.
    // Use a combined hash of the fingerprint fields for the subtree hash.
    let node_self_hash = {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut h = DefaultHasher::new();
        new_fingerprint.hash(&mut h);
        h.finish()
    };
    let final_subtree_hash = calculate_subtree_hash(node_self_hash, &new_child_hashes);
    if let Some(current_node) = new_tree_builder.get_mut(new_node_idx) {
        current_node.subtree_hash = final_subtree_hash;
    }

    // Classify this node into the appropriate dirty set based on what changed.
    if dirty_flag >= DirtyFlag::Layout || children_are_different {
        recon.intrinsic_dirty.insert(new_node_idx);
        recon.layout_roots.insert(new_node_idx);
    } else if dirty_flag == DirtyFlag::Paint {
        recon.paint_dirty.insert(new_node_idx);
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
        ctx.viewport_size,
    )?;

    // Phase 2: Layout children using a formatting context
    // Use pre-computed styles from LayoutNode instead of repeated lookups
    let writing_mode = node.computed_style.writing_mode;
    let text_align = node.computed_style.text_align;
    let display = node.computed_style.display;
    let overflow_y = node.computed_style.overflow_y;

    // Check if height is auto (no explicit height set)
    let height_is_auto = node.computed_style.height.is_none();

    let available_size_for_children = if height_is_auto {
        // Height is auto - use containing block size as available size
        let inner_size = node.box_props.inner_size(final_used_size, writing_mode);

        // For inline elements (display: inline), the available width comes from
        // the containing block, not from the element's own intrinsic size.
        // CSS 2.2 § 10.3.1: Inline, non-replaced elements use containing block width.
        let available_width = match display {
            LayoutDisplay::Inline => containing_block_size.width,
            _ => inner_size.width,
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

    // NOTE: Scrollbar reservation is handled inside layout_bfc() where it subtracts
    // scrollbar width from children_containing_block_size. We do NOT subtract here
    // to avoid double-subtraction (layout_bfc already handles both the used_size
    // and available_size code paths).

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
        return ScrollbarRequirements::default();
    }

    let overflow_x = get_overflow_x(ctx.styled_dom, dom_id, styled_node_state);
    let overflow_y = get_overflow_y(ctx.styled_dom, dom_id, styled_node_state);

    let container_size = box_props.inner_size(final_used_size, writing_mode);

    // Resolve per-node scrollbar width from CSS + OS overlay preference
    let scrollbar_width_px =
        crate::solver3::getters::get_layout_scrollbar_width_px(ctx, dom_id, styled_node_state);

    fc::check_scrollbar_necessity(
        content_size,
        container_size,
        to_overflow_behavior(overflow_x),
        to_overflow_behavior(overflow_y),
        scrollbar_width_px,
    )
}

/// Checks if scrollbars changed compared to previous layout and if reflow is needed.
///
/// Detects both addition AND removal of scrollbars. Oscillation (add → remove → add)
/// is prevented by the outer layout loop's iteration limit (`loop_count > 10` in mod.rs),
/// not by suppressing removal detection here. This allows scrollbars to correctly
/// disappear when content shrinks or the window is resized larger.
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
            // Trigger reflow if scrollbar state changed in either direction
            let horizontal_changed = old_info.needs_horizontal != scrollbar_info.needs_horizontal;
            let vertical_changed = old_info.needs_vertical != scrollbar_info.needs_vertical;
            horizontal_changed || vertical_changed
        }
    }
}

/// Returns the new scrollbar info directly, replacing any previous state.
///
/// Previous versions used `||` to make scrollbars "sticky" (never removed once added).
/// This prevented oscillation but caused scrollbars to persist forever—even after
/// content shrinks or the window grows. The outer layout loop's iteration cap
/// now handles oscillation safety, so we can faithfully reflect the current state.
fn merge_scrollbar_info(
    _tree: &LayoutTree,
    _node_index: usize,
    new_info: &ScrollbarRequirements,
) -> ScrollbarRequirements {
    new_info.clone()
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
    // Always print positioning info for debugging
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

    let Some(debug_msgs) = ctx.debug_messages.as_mut() else {
        return;
    };

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
/// For Block/Inline/Table, layout_bfc/layout_ifc already laid out children in Pass 1.
/// We only need to set absolute positions and recurse for positioning grandchildren.
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
    calculated_positions: &mut super::PositionVec,
    reflow_needed_for_scrollbars: &mut bool,
    float_cache: &mut BTreeMap<usize, fc::FloatingContext>,
) -> Result<()> {
    // Set relative position on child
    // child_relative_pos is [CoordinateSpace::Parent] - relative to parent's content-box
    let child_node = tree.get_mut(child_index).ok_or(LayoutError::InvalidTree)?;
    child_node.relative_position = Some(child_relative_pos);

    // Calculate absolute position
    // self_content_box_pos is [CoordinateSpace::Window] - absolute position of parent's content-box
    // child_absolute_pos becomes [CoordinateSpace::Window] - absolute window position of child
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

    // calculated_positions stores [CoordinateSpace::Window] - absolute positions
    super::pos_set(calculated_positions, child_index, child_absolute_pos);

    // Get child's properties for recursion
    let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
    let child_content_box_pos =
        calculate_content_box_pos(child_absolute_pos, &child_node.box_props);
    let child_inner_size = child_node
        .box_props
        .inner_size(child_node.used_size.unwrap_or_default(), writing_mode);
    let child_children: Vec<usize> = child_node.children.clone();
    let child_fc = child_node.formatting_context.clone();

    // Recurse to position grandchildren
    // OPTIMIZATION: For BFC/IFC children, layout_bfc/layout_ifc already computed their layout.
    // We just need to set absolute positions for descendants.
    // Only recurse if child has children to position.
    if !child_children.is_empty() {
        if is_flex_or_grid {
            // For Flex/Grid: Taffy already set used_size. Only recurse for grandchildren.
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
            // For Block/Inline/Table: The formatting context already laid out children.
            // Recursively position grandchildren using their cached layout data.
            position_bfc_child_descendants(
                tree,
                child_index,
                child_content_box_pos,
                calculated_positions,
            );
        }
    }

    Ok(())
}

/// Recursively positions descendants of a BFC/IFC child without re-computing layout.
/// The layout was already computed by layout_bfc/layout_ifc.
/// We only need to convert relative positions to absolute positions.
fn position_bfc_child_descendants(
    tree: &LayoutTree,
    node_index: usize,
    content_box_pos: LogicalPosition,
    calculated_positions: &mut super::PositionVec,
) {
    let Some(node) = tree.get(node_index) else { return };
    
    for &child_index in &node.children {
        let Some(child_node) = tree.get(child_index) else { continue };
        
        // Use the relative_position that was set during formatting context layout
        let child_rel_pos = child_node.relative_position.unwrap_or_default();
        let child_abs_pos = LogicalPosition::new(
            content_box_pos.x + child_rel_pos.x,
            content_box_pos.y + child_rel_pos.y,
        );
        
        super::pos_set(calculated_positions, child_index, child_abs_pos);
        
        // Calculate child's content-box position for recursion
        let child_content_box_pos = LogicalPosition::new(
            child_abs_pos.x + child_node.box_props.border.left + child_node.box_props.padding.left,
            child_abs_pos.y + child_node.box_props.border.top + child_node.box_props.padding.top,
        );
        
        // Recurse to grandchildren
        position_bfc_child_descendants(tree, child_index, child_content_box_pos, calculated_positions);
    }
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
    calculated_positions: &mut super::PositionVec,
) -> Result<()> {
    // Collect out-of-flow children (those not already positioned)
    let out_of_flow_children: Vec<(usize, Option<NodeId>)> = {
        let current_node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        current_node
            .children
            .iter()
            .filter_map(|&child_index| {
                if super::pos_contains(calculated_positions, child_index) {
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
        super::pos_set(calculated_positions, child_index, self_content_box_pos);

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
///
/// Uses the per-node multi-slot cache (inspired by Taffy's 9+1 architecture) to
/// avoid O(n²) complexity. Each node has 9 measurement slots + 1 full layout slot.
///
/// ## Two-Mode Architecture (CSS Two-Pass Layout)
///
/// `compute_mode` determines behavior:
///
/// - **`ComputeSize`** (BFC Pass 1 — sizing):
///   Computes only the node's border-box size. On cache hit from measurement slots,
///   sets `used_size` and returns immediately — no child positioning. This is the
///   key to O(n) two-pass BFC: Pass 1 fills measurement caches cheaply.
///
/// - **`PerformLayout`** (BFC Pass 2 — positioning):
///   Computes size AND positions all children. On cache hit from layout slot,
///   applies cached child positions recursively. When Pass 2 provides the same
///   constraints as Pass 1, the "result matches request" optimization triggers
///   automatic cache hits.
///
/// ## Cache Hit Rates (Taffy's "result matches request" optimization)
///
/// When Pass 1 measures a node with available_size A and gets result_size R,
/// then Pass 2 provides R as a known_dimension, `get_size()` / `get_layout()`
/// recognize R == cached.result_size as a cache hit. This is the fundamental
/// mechanism ensuring O(n) total complexity across both passes.
pub fn calculate_layout_for_subtree<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    containing_block_pos: LogicalPosition,
    containing_block_size: LogicalSize,
    calculated_positions: &mut super::PositionVec,
    reflow_needed_for_scrollbars: &mut bool,
    float_cache: &mut BTreeMap<usize, fc::FloatingContext>,
    compute_mode: ComputeMode,
) -> Result<()> {
    // === PER-NODE CACHE CHECK (Taffy-inspired 9+1 slot cache) ===
    //
    // Two-mode cache lookup (CSS two-pass architecture):
    //
    // ComputeSize (Pass 1 — sizing):
    //   1. Check measurement slots (get_size) → if hit, set used_size and return.
    //      No child positioning needed — we only need the node's border-box size.
    //   2. Fall back to layout slot → if hit, extract size from full layout result.
    //
    // PerformLayout (Pass 2 — positioning):
    //   1. Check layout slot (get_layout) → if hit, apply cached child positions.
    //   2. No fallback to measurement slots (we need full positions, not just size).
    //
    // This split is critical for O(n) two-pass BFC:
    // - Pass 1 populates measurement slots (cheap: no absolute positioning)
    // - Pass 2 hits layout slot or re-computes with positions
    if node_index < ctx.cache_map.entries.len() {
        match compute_mode {
            ComputeMode::ComputeSize => {
                // ComputeSize: check measurement slot first (Taffy's 9-slot scheme)
                let sizing_hit = ctx.cache_map.entries[node_index]
                    .get_size(0, containing_block_size)
                    .cloned();
                if let Some(cached_sizing) = sizing_hit {
                    // SIZING CACHE HIT — set used_size and return immediately.
                    // No child positioning needed in ComputeSize mode.
                    if let Some(node) = tree.get_mut(node_index) {
                        node.used_size = Some(cached_sizing.result_size);
                        node.escaped_top_margin = cached_sizing.escaped_top_margin;
                        node.escaped_bottom_margin = cached_sizing.escaped_bottom_margin;
                        node.baseline = cached_sizing.baseline;
                    }
                    return Ok(());
                }
                // Fall through to layout slot check
                let layout_hit = ctx.cache_map.entries[node_index]
                    .get_layout(containing_block_size)
                    .cloned();
                if let Some(cached_layout) = layout_hit {
                    // Layout slot hit in ComputeSize mode — extract size only
                    if let Some(node) = tree.get_mut(node_index) {
                        node.used_size = Some(cached_layout.result_size);
                        node.overflow_content_size = Some(cached_layout.content_size);
                        node.scrollbar_info = Some(cached_layout.scrollbar_info.clone());
                    }
                    return Ok(());
                }
            }
            ComputeMode::PerformLayout => {
                // PerformLayout: check layout slot (the single "full layout" slot)
                let layout_hit = ctx.cache_map.entries[node_index]
                    .get_layout(containing_block_size)
                    .cloned();
                if let Some(cached_layout) = layout_hit {
                    // LAYOUT CACHE HIT — apply cached results with child positions
                    if let Some(node) = tree.get_mut(node_index) {
                        node.used_size = Some(cached_layout.result_size);
                        node.overflow_content_size = Some(cached_layout.content_size);
                        node.scrollbar_info = Some(cached_layout.scrollbar_info.clone());
                    }

                    let box_props = tree.get(node_index)
                        .map(|n| n.box_props.clone())
                        .unwrap_or_default();
                    let self_content_box_pos = calculate_content_box_pos(containing_block_pos, &box_props);

                    // Apply cached child positions and recurse
                    let result_size = cached_layout.result_size;
                    for (child_index, child_relative_pos) in &cached_layout.child_positions {
                        let child_abs_pos = LogicalPosition::new(
                            self_content_box_pos.x + child_relative_pos.x,
                            self_content_box_pos.y + child_relative_pos.y,
                        );
                        super::pos_set(calculated_positions, *child_index, child_abs_pos);

                        let inner = box_props.inner_size(
                            result_size,
                            LayoutWritingMode::HorizontalTb,
                        );
                        // Subtract scrollbar reservation from the available size
                        // passed to children. This mirrors what layout_bfc does in
                        // the MISS path — without it, a reflow-loop cache hit
                        // would hand children the full content-box width, ignoring
                        // any vertical/horizontal scrollbar that was detected.
                        let child_available_size =
                            cached_layout.scrollbar_info.shrink_size(inner);
                        calculate_layout_for_subtree(
                            ctx,
                            tree,
                            text_cache,
                            *child_index,
                            child_abs_pos,
                            child_available_size,
                            calculated_positions,
                            reflow_needed_for_scrollbars,
                            float_cache,
                            compute_mode,
                        )?;
                    }

                    return Ok(());
                }
            }
        }
    }
    
    // === CACHE MISS — compute layout ===
    
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

    // Phase 1.5: Update used_size BEFORE calling layout_formatting_context.
    //
    // When a node is cloned from the old tree (clone_node_from_old), its used_size
    // retains the value from the previous layout pass. If the containing block changed
    // (e.g. viewport resize), the stale used_size would cause layout_bfc() to compute
    // an incorrect children_containing_block_size. By updating used_size here, we ensure
    // that layout_bfc reads the freshly resolved size from prepare_layout_context.
    {
        let is_table_cell = tree.get(node_index).map_or(false, |n| {
            matches!(n.formatting_context, FormattingContext::TableCell)
        });
        if !is_table_cell {
            if let Some(node) = tree.get_mut(node_index) {
                node.used_size = Some(final_used_size);
            }
        }
    }

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

    // Check if this node is a scroll container (overflow: scroll/auto).
    // Scroll containers must NOT expand to fit content — their height is
    // determined by the containing block, and overflow is scrollable.
    //
    // Exception: if the containing block height is infinite (unconstrained),
    // we must still grow, since you can't scroll inside an infinitely tall box.
    let is_scroll_container = dom_id.map_or(false, |id| {
        let ov_x = get_overflow_x(ctx.styled_dom, id, &styled_node_state);
        let ov_y = get_overflow_y(ctx.styled_dom, id, &styled_node_state);
        matches!(ov_x, MultiValue::Exact(LayoutOverflow::Scroll) | MultiValue::Exact(LayoutOverflow::Auto))
            || matches!(ov_y, MultiValue::Exact(LayoutOverflow::Scroll) | MultiValue::Exact(LayoutOverflow::Auto))
    });

    if should_use_content_height(&css_height) {
        let skip_expansion = is_scroll_container
            && containing_block_size.height.is_finite()
            && containing_block_size.height > 0.0;

        if !skip_expansion {
            final_used_size = apply_content_based_height(
                final_used_size,
                content_size,
                tree,
                node_index,
                writing_mode,
            );
        }
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
        current_node.scrollbar_info = Some(merged_scrollbar_info.clone());
        // Store overflow content size for scroll frame calculation
        current_node.overflow_content_size = Some(content_size);

        // self_content_box_pos is [CoordinateSpace::Window] - the absolute position of this node's content-box
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
    // Positions in layout_result.output.positions are [CoordinateSpace::Parent] - relative to this node's content-box
    let positions: Vec<_> = layout_result
        .output
        .positions
        .iter()
        .map(|(&idx, &pos)| (idx, pos))
        .collect();

    // Store child positions for cache
    let child_positions_for_cache: Vec<(usize, LogicalPosition)> = positions.clone();

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

    // === STORE RESULT IN PER-NODE CACHE (Taffy-inspired 9+1 slot cache) ===
    // Store both the full layout entry and a sizing measurement entry.
    // This enables O(n) two-pass BFC: Pass 1 populates cache, Pass 2 reads it.
    if node_index < ctx.cache_map.entries.len() {
        let node_ref = tree.get(node_index);
        let baseline = node_ref.and_then(|n| n.baseline);
        let escaped_top = node_ref.and_then(|n| n.escaped_top_margin);
        let escaped_bottom = node_ref.and_then(|n| n.escaped_bottom_margin);

        // Store in the layout slot (PerformLayout result)
        ctx.cache_map.get_mut(node_index).store_layout(LayoutCacheEntry {
            available_size: containing_block_size,
            result_size: final_used_size,
            content_size,
            child_positions: child_positions_for_cache.clone(),
            escaped_top_margin: escaped_top,
            escaped_bottom_margin: escaped_bottom,
            scrollbar_info: merged_scrollbar_info.clone(),
        });

        // Also store in a measurement slot (slot 0: both dimensions known)
        // This enables the "result matches request" optimization (Taffy pattern):
        // when Pass 2 provides the same size as Pass 1 measured, it's a cache hit.
        ctx.cache_map.get_mut(node_index).store_size(0, SizingCacheEntry {
            available_size: containing_block_size,
            result_size: final_used_size,
            baseline,
            escaped_top_margin: escaped_top,
            escaped_bottom_margin: escaped_bottom,
        });
    }

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
    calculated_positions: &mut super::PositionVec,
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

            // Insert position
            super::pos_set(calculated_positions, child_index, child_abs_pos);

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
            super::pos_set(calculated_positions, child_index, child_abs_pos);

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
    calculated_positions: &mut super::PositionVec,
) -> Result<()> {
    let out_of_flow_children: Vec<(usize, Option<NodeId>)> = {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        node.children
            .iter()
            .filter_map(|&child_index| {
                if super::pos_contains(calculated_positions, child_index) {
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
                super::pos_set(calculated_positions, child_index, parent_content_box_pos);

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
            LayoutHeight::Calc(_) => {
                // Calc expressions are not auto, they compute to a specific value
                false
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

// hash_styled_node_data() removed — replaced by NodeDataFingerprint::compute()

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
    let display = {
        use crate::solver3::getters::get_display_property;
        get_display_property(styled_dom, Some(dom_id)).exact()
    };
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
