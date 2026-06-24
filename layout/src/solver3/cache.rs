//! Handling Viewport Resizing and Layout Thrashing
//!
//! The viewport size is a fundamental input to the entire layout process.
//! A change in viewport size must trigger a relayout.
//!
//! 1. The `layout_document` function takes the `viewport` as an argument. The `LayoutCache` stores
//!    the `viewport` from the previous frame.
//! 2. The `reconcile_and_invalidate` function detects that the viewport has changed size
//! 3. This single change—marking the root as a layout root—forces a full top-down pass
//!    (`calculate_layout_for_subtree` starting from the root). This correctly recalculates all
//!    percentage-based sizes and repositions all elements according to the new viewport dimensions.
//! 4. The intrinsic size calculation (bottom-up) can often be skipped, as it's independent of the
//!    container size, which is a significant optimization.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    hash::{DefaultHasher, Hash, Hasher},
};

/// Floating-point comparison epsilon for cache size lookups.
/// Controls the tolerance for cache hit matching in the per-node multi-slot cache.
const CACHE_SIZE_EPSILON: f32 = 0.1;

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
            LayoutDisplay, LayoutHeight, LayoutOverflow,
            LayoutPosition, LayoutWritingMode,
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
            get_css_height, get_display_property, get_overflow_x,
            get_overflow_y, get_scrollbar_gutter_property, get_text_align, get_white_space_property, get_writing_mode,
            MultiValue,
        },
        layout_tree::{
            get_display_type, is_block_level, AnonymousBoxType, DirtyFlag, LayoutNode, LayoutNodeHot, LayoutTreeBuilder, SubtreeHash,
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
/// classic CSS two-pass layout: Pass 1 (`ComputeSize`) measures all children,
/// Pass 2 (`PerformLayout`) positions them using the measured sizes.
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
/// different cache slot, preventing collisions between e.g. `MinContent` and
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

/// Cache entry for sizing (`ComputeSize` mode) — stores NO positions.
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

/// Cache entry for full layout (`PerformLayout` mode).
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
/// clobber each other (unlike the old global `BTreeMap` where fixed-point
/// collisions were possible).
///
/// NOT stored on `LayoutNode` — lives in the external `LayoutCacheMap`.
#[derive(Debug, Clone)]
pub struct NodeCache {
    /// 9 measurement slots (Taffy's deterministic scheme):
    /// - Slot 0: both dimensions known
    /// - Slots 1-2: only width known (MaxContent/Definite vs `MinContent`)
    /// - Slots 3-4: only height known (MaxContent/Definite vs `MinContent`)
    /// - Slots 5-8: neither known (2×2 combos of width/height constraint types)
    pub measure_entries: [Option<SizingCacheEntry>; 9],

    /// 1 full layout slot (with child positions, overflow, baseline).
    /// Only populated after `PerformLayout`, not after `ComputeSize`.
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
    ///
    /// TODO(superplan): currently unused — the layout cache only ever touches
    /// slot 0 (see the `get_size(0, ..)` / `store_size(0, ..)` call sites). This
    /// is the intended entry point for wiring the full 9-slot scheme.
    #[must_use] pub fn slot_index(
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
                let w = usize::from(width_type == AvailableWidthType::MinContent);
                let h = usize::from(height_type == AvailableWidthType::MinContent);
                5 + w * 2 + h
            }
        }
    }

    /// Look up a sizing cache entry, implementing Taffy's "result matches request"
    /// optimization: if the caller provides the result size as a known dimension
    /// (common in Pass1→Pass2 transitions), it's still a cache hit.
    #[must_use] pub fn get_size(&self, slot: usize, known_dims: LogicalSize) -> Option<&SizingCacheEntry> {
        let entry = self.measure_entries[slot].as_ref()?;
        // Exact match on input constraints
        if (known_dims.width - entry.available_size.width).abs() < CACHE_SIZE_EPSILON
            && (known_dims.height - entry.available_size.height).abs() < CACHE_SIZE_EPSILON
        {
            return Some(entry);
        }
        // "Result matches request" — if the caller provides the result size
        // as a known dimension, it's still a hit. This is the key optimization
        // that makes two-pass layout O(n): Pass 1 measures a node, Pass 2
        // provides the measured size as a constraint → automatic cache hit.
        if (known_dims.width - entry.result_size.width).abs() < CACHE_SIZE_EPSILON
            && (known_dims.height - entry.result_size.height).abs() < CACHE_SIZE_EPSILON
        {
            return Some(entry);
        }
        None
    }

    /// Store a sizing result in the given slot.
    pub const fn store_size(&mut self, slot: usize, entry: SizingCacheEntry) {
        self.measure_entries[slot] = Some(entry);
        self.is_empty = false;
    }

    /// Look up the full layout cache entry.
    #[must_use] pub fn get_layout(&self, known_dims: LogicalSize) -> Option<&LayoutCacheEntry> {
        let entry = self.layout_entry.as_ref()?;
        if (known_dims.width - entry.available_size.width).abs() < CACHE_SIZE_EPSILON
            && (known_dims.height - entry.available_size.height).abs() < CACHE_SIZE_EPSILON
        {
            return Some(entry);
        }
        // "Result matches request" for layout too
        if (known_dims.width - entry.result_size.width).abs() < CACHE_SIZE_EPSILON
            && (known_dims.height - entry.result_size.height).abs() < CACHE_SIZE_EPSILON
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
/// - O(1) indexed lookup (Vec) instead of O(log n) (`BTreeMap`)
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
    #[must_use] pub fn get(&self, node_index: usize) -> &NodeCache {
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
    pub fn mark_dirty(&mut self, node_index: usize, tree: &[LayoutNodeHot]) {
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
    /// Stable scroll IDs computed from `node_data_hash` (layout index -> scroll ID)
    pub scroll_ids: HashMap<usize, u64>,
    /// Mapping from scroll ID to DOM `NodeId` for hit testing
    pub scroll_id_to_node_id: HashMap<u64, NodeId>,
    /// CSS counter values for each node and counter name.
    /// Key: (`layout_index`, `counter_name`), Value: counter value
    /// This stores the computed counter values after processing counter-reset and
    /// counter-increment.
    pub counters: HashMap<(usize, String), i32>,
    /// Cache of positioned floats for each BFC node (`layout_index` -> `FloatingContext`).
    /// This persists float positions across multiple layout passes, ensuring IFC
    /// children always have access to correct float exclusions even when layout is
    /// recalculated.
    pub float_cache: HashMap<usize, fc::FloatingContext>,
    /// Per-node multi-slot cache (inspired by Taffy's 9+1 architecture).
    /// External to `LayoutTree` — indexed by node index for O(1) lookup.
    /// Persists across frames; resized after reconciliation.
    pub cache_map: LayoutCacheMap,
    /// Snapshot of `calculated_positions` from the previous frame, used by the
    /// compositor to compute damage rects (old bounds vs new bounds).
    pub previous_positions: super::PositionVec,
    /// Cached display list keyed by `(root_subtree_hash, viewport)`.
    /// When the reconciled tree has the same root `subtree_hash` AND
    /// the same viewport as the cached one, the display list is
    /// returned as-is — skipping layout, positioning, and
    /// display-list generation entirely. Cleared whenever
    /// `mark_dirty` fires on any node (since the root's upstream
    /// invalidation chain clears its ancestors).
    pub cached_display_list: Option<(SubtreeHash, LogicalRect, super::display_list::DisplayList)>,
    /// Raw pointer of the `StyledDom` from the previous layout pass. When the
    /// same `&StyledDom` reference is passed again AND the viewport is unchanged,
    /// skip reconcile entirely and return the cached display list (saves ~0.8 ms).
    pub prev_dom_ptr: usize,
    pub prev_viewport: LogicalRect,
}

/// Approximate heap-byte breakdown of the solver3 `LayoutCache`.
#[derive(Debug, Clone, Default)]
pub struct Solver3CacheMemoryReport {
    pub tree_bytes: usize,
    pub tree_report: Option<super::layout_tree::LayoutTreeMemoryReport>,
    pub calculated_positions_bytes: usize,
    pub previous_positions_bytes: usize,
    pub scroll_ids_bytes: usize,
    pub scroll_id_to_node_id_bytes: usize,
    pub counters_bytes: usize,
    pub float_cache_bytes: usize,
    pub cache_map_bytes: usize,
    pub cached_display_list_bytes: usize,
}

impl Solver3CacheMemoryReport {
    #[must_use] pub const fn total_bytes(&self) -> usize {
        self.tree_bytes
            + self.calculated_positions_bytes
            + self.previous_positions_bytes
            + self.scroll_ids_bytes
            + self.scroll_id_to_node_id_bytes
            + self.counters_bytes
            + self.float_cache_bytes
            + self.cache_map_bytes
            + self.cached_display_list_bytes
    }
}

impl LayoutCache {
    /// Drop all incremental-reuse state so the next `layout_document` lays the
    /// DOM out from scratch (cold path), as if no previous frame existed.
    ///
    /// Required before laying out a DOM whose `NodeIds` are NOT a stable evolution
    /// of whatever this (shared) cache last held — namely `VirtualView` / iframe
    /// child DOMs, which their callbacks rebuild wholesale on every invocation.
    /// Incremental reconciliation matches/reuses subtrees by `NodeId` + subtree
    /// hash; on a wholesale rebuild those `NodeIds` are reassigned, so reusing the
    /// prior tree can graft `NodeIds` that no longer exist in the new `StyledDom`
    /// (panic: out-of-bounds `node_data` index when the DOM shrinks — e.g. the map
    /// dropping tiles on zoom-out).
    pub fn reset_incremental(&mut self) {
        self.tree = None;
        self.cache_map = LayoutCacheMap::default();
        self.cached_display_list = None;
        self.prev_dom_ptr = 0;
        self.counters.clear();
        self.float_cache.clear();
    }

    /// Approximate heap bytes retained by this `LayoutCache`.
    #[must_use] pub fn memory_report(&self) -> Solver3CacheMemoryReport {
        let tree_report = self.tree.as_ref().map(LayoutTree::memory_report);
        let tree_bytes = tree_report.as_ref().map_or(0, super::layout_tree::LayoutTreeMemoryReport::total_bytes);
        // cache_map: Vec<NodeCache>; NodeCache has 9 Option<SizingCacheEntry>
        // + 1 Option<LayoutCacheEntry>. Count filled layout entries' child_positions.
        let mut cache_map_bytes = self.cache_map.entries.capacity()
            * size_of::<NodeCache>();
        for e in &self.cache_map.entries {
            if let Some(le) = &e.layout_entry {
                cache_map_bytes += le.child_positions.capacity()
                    * size_of::<(usize, LogicalPosition)>();
            }
        }
        Solver3CacheMemoryReport {
            tree_bytes,
            tree_report,
            calculated_positions_bytes: self.calculated_positions.len()
                * size_of::<LogicalPosition>(),
            previous_positions_bytes: self.previous_positions.len()
                * size_of::<LogicalPosition>(),
            scroll_ids_bytes: self.scroll_ids.len()
                * (size_of::<usize>() + size_of::<u64>()),
            scroll_id_to_node_id_bytes: self.scroll_id_to_node_id.len()
                * (size_of::<u64>() + size_of::<NodeId>()),
            counters_bytes: self.counters.iter().map(|((_, name), _)| {
                size_of::<(usize, String)>()
                    + size_of::<i32>()
                    + name.capacity()
            }).sum(),
            float_cache_bytes: self.float_cache.len() * 256, // conservative per-FC
            cache_map_bytes,
            cached_display_list_bytes: if self.cached_display_list.is_some() { 2048 } else { 0 },
        }
    }
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
    #[must_use] pub fn is_clean(&self) -> bool {
        self.intrinsic_dirty.is_empty()
            && self.layout_roots.is_empty()
            && self.paint_dirty.is_empty()
    }

    /// Returns true if full layout work is needed for at least one node.
    #[must_use] pub fn needs_layout(&self) -> bool {
        !self.intrinsic_dirty.is_empty() || !self.layout_roots.is_empty()
    }

    /// Returns true if only paint work is needed (no layout).
    #[must_use] pub fn needs_paint_only(&self) -> bool {
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
        let Some(parent_node) = tree.get(parent_idx) else {
            continue;
        };

        // Dispatch to the correct repositioning logic based on the parent's layout mode.
        match parent_node.formatting_context {
            // Cases that use simple block-flow stacking can be optimized.
            FormattingContext::Block { .. } | FormattingContext::TableRowGroup => {
                reposition_block_flow_siblings(
                    styled_dom,
                    parent_idx,
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
                // TODO: Table layout is interdependent. A change in one cell's size
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

/// Convert `LayoutOverflow` to `OverflowBehavior`
/// CSS Overflow Module Level 3: initial value of `overflow` is `visible`.
// +spec:overflow:3a6297 - initial value 'visible', maps hidden/scroll/auto overflow behaviors
fn to_overflow_behavior(overflow: MultiValue<LayoutOverflow>) -> fc::OverflowBehavior {
    match overflow.unwrap_or(LayoutOverflow::Visible) {
        LayoutOverflow::Visible => fc::OverflowBehavior::Visible,
        LayoutOverflow::Hidden | LayoutOverflow::Clip => fc::OverflowBehavior::Hidden,
        LayoutOverflow::Scroll => fc::OverflowBehavior::Scroll,
        LayoutOverflow::Auto => fc::OverflowBehavior::Auto,
    }
}

/// Convert `StyleTextAlign` to `fc::TextAlign`
// +spec:text-alignment-spacing:43ea0a - text-align-all shorthand: aligns all lines except last (overridden by text-align-last)
const fn style_text_align_to_fc(text_align: StyleTextAlign) -> fc::TextAlign {
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
/// Children with `display: none` are filtered out since they generate no boxes.
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/counter/fixed-point cast
#[must_use] pub fn collect_children_dom_ids(styled_dom: &StyledDom, parent_dom_id: NodeId) -> Vec<NodeId> {
    let hierarchy_container = styled_dom.node_hierarchy.as_container();
    let mut children = Vec::new();

    let Some(hierarchy_item) = hierarchy_container.get(parent_dom_id) else {
        return children;
    };

    let Some(mut child_id) = hierarchy_item.first_child_id(parent_dom_id) else {
        // DEBUG (2026-06-02 children-None): first_child_id returned None for this
        // parent → 0xC0000000 marker @0x40540+parent*4. REVERT before commit.
        unsafe {
            let pi = parent_dom_id.index();
            if pi < 8 { crate::az_mark((0x40540 + pi * 4) as u32, (0xC000_0000u32)); }
        }
        return children;
    };

    // +spec:display-property:9f02c6 - display:none elements generate no boxes
    // +spec:display-property:3b507e - display:none excludes subtree from box tree
    if get_display_type(styled_dom, child_id) != LayoutDisplay::None {
        children.push(child_id);
    }
    while let Some(hierarchy_item) = hierarchy_container.get(child_id) {
        let Some(next) = hierarchy_item.next_sibling_id() else {
            break;
        };
        if get_display_type(styled_dom, next) != LayoutDisplay::None {
            children.push(next);
        }
        child_id = next;
    }

    // DEBUG (2026-06-02 children-None): record collected child count per parent
    // @0x40540+parent*4 (0xCC00_00NN). N=0 with first_child Some ⇒ get_display_type
    // mis-lift skipped them; N>0 ⇒ walk works. REVERT before commit.
    unsafe {
        let pi = parent_dom_id.index();
        if pi < 8 {
            crate::az_mark((0x40540 + pi * 4) as u32, (0xCC00_0000u32 | (children.len() as u32 & 0xffff)));
        }
    }
    children
}

/// Repositions clean children within a simple block-flow layout (like a BFC or a
/// table-row-group). It stacks children along the main axis, preserving their
/// previously calculated cross-axis alignment.
pub fn reposition_block_flow_siblings(
    styled_dom: &StyledDom,
    parent_idx: usize,
    tree: &LayoutTree,
    layout_roots: &BTreeSet<usize>,
    calculated_positions: &mut super::PositionVec,
) {
    let Some(parent_node) = tree.get(parent_idx) else {
        return;
    };
    let dom_id = parent_node.dom_node_id.unwrap_or(NodeId::ZERO);
    let styled_node_state = styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| n.styled_node_state)
        .unwrap_or_default();

    let writing_mode = get_writing_mode(styled_dom, dom_id, &styled_node_state).unwrap_or_default();

    let parent_pos = calculated_positions
        .get(parent_idx)
        .copied()
        .unwrap_or_default();

    let parent_bp = parent_node.box_props.unpack();
    let content_box_origin = LogicalPosition::new(
        parent_pos.x + parent_bp.padding.left,
        parent_pos.y + parent_bp.padding.top,
    );

    let mut main_pen = 0.0;

    for &child_idx in tree.children(parent_idx) {
        let Some(child_node) = tree.get(child_idx) else {
            continue;
        };

        let child_size = child_node.used_size.unwrap_or_default();
        let child_bp = child_node.box_props.unpack();
        let child_main_sum = child_bp.margin.main_sum(writing_mode);
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
                + child_bp.margin.main_end(writing_mode);
        } else {
            // This child is *clean*. Calculate its new position and shift its
            // entire subtree.
            let old_pos = match calculated_positions.get(child_idx) {
                Some(p) => *p,
                None => continue,
            };

            let child_main_start = child_bp.margin.main_start(writing_mode);
            let new_main_pos = main_pen + child_main_start;
            let old_relative_pos = tree.warm(child_idx)
                .and_then(|w| w.relative_position)
                .unwrap_or_default();
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
fn shift_subtree_position(
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
        let children = tree.children(node_idx).to_vec();
        for &child_idx in &children {
            shift_subtree_position(child_idx, delta, tree, calculated_positions);
        }
    }
}

/// Compares the new DOM against the cached tree, creating a new tree
/// and identifying which parts need to be re-laid out.
/// Count how many of the supplied DOM children would actually end up
/// in the layout tree. Mirrors the filters applied by
/// `LayoutTreeBuilder::build_recursive` so reconciliation can compare
/// like-for-like:
///
/// - `display: none` nodes are skipped entirely.
/// - In table structural contexts (table, row-group, row) whitespace
///   text nodes are skipped (CSS 2.2 §17.2.1, matches
///   `should_skip_for_table_structure`).
/// - Whitespace-only inline runs that sit between block siblings
///   collapse to zero boxes (CSS 2.2 §9.2.2.1).
///
/// The first two rules drop children unconditionally; the third only
/// fires on siblings surrounding a block-level child, so we detect it
/// by walking the run pairs. We do not build the runs — just count
/// survivors.
fn layout_relevant_child_count(
    styled_dom: &StyledDom,
    children: &[NodeId],
    parent_id: NodeId,
) -> usize {
    use super::getters::{get_display_property, MultiValue};
    use super::layout_tree::{is_block_level, is_whitespace_only_text};

    let parent_display = match get_display_property(styled_dom, Some(parent_id)) {
        MultiValue::Exact(d) => d,
        _ => LayoutDisplay::Block,
    };
    let is_table_structural = matches!(
        parent_display,
        LayoutDisplay::Table
            | LayoutDisplay::InlineTable
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
            | LayoutDisplay::TableRow
    );

    let has_any_block_child = children
        .iter()
        .any(|&id| is_block_level(styled_dom, id));

    let mut count = 0usize;
    // When parent has any block child, whitespace-only inline runs
    // surrounding blocks collapse. We approximate that by skipping
    // whitespace text whenever any block sibling exists.
    let collapse_inline_whitespace = has_any_block_child;
    for &id in children {
        // display:none drops
        let display = match get_display_property(styled_dom, Some(id)) {
            MultiValue::Exact(d) => d,
            _ => LayoutDisplay::Block,
        };
        if matches!(display, LayoutDisplay::None) {
            continue;
        }
        // Table-structural whitespace drops.
        if is_table_structural && is_whitespace_only_text(styled_dom, id) {
            continue;
        }
        // Whitespace-only inline run collapse when mixed with blocks.
        if collapse_inline_whitespace
            && !is_block_level(styled_dom, id)
            && is_whitespace_only_text(styled_dom, id)
        {
            continue;
        }
        count += 1;
    }
    count
}

pub fn reconcile_and_invalidate<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    cache: &LayoutCache,
    viewport: LogicalRect,
) -> Result<(LayoutTree, ReconciliationResult)> {
    let _probe_outer = crate::probe::Probe::span("reconcile_and_invalidate");
    let mut new_tree_builder = LayoutTreeBuilder::new(ctx.viewport_size);
    let mut recon_result = ReconciliationResult::default();
    // A viewport SIZE change invalidates every computed size: percentage, flex,
    // and absolute insets (top/right/bottom/left) all resolve against the
    // viewport / containing block. Incrementally reusing the cached layout tree
    // left out-of-flow and VirtualView nodes sized against the OLD viewport — e.g.
    // the map's absolutely-positioned container kept its old size, so a maximized
    // window showed tiles only in the original rect and grey everywhere else
    // (#9 "grey on resize"). On a size change, drop the cached tree so the whole
    // tree is laid out fresh against the new viewport. (Position-only moves keep
    // the incremental path.)
    let viewport_resized = cache.viewport.is_none_or(|v| v.size != viewport.size);
    let old_tree = if viewport_resized {
        None
    } else {
        cache.tree.as_ref()
    };

    if viewport_resized {
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
        ctx.debug_messages,
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
    // layout_document's step marker is stuck at 1 (post-`?` not reached), the
    // lifted `?` mis-discriminated this Ok as Err (niche-Result mis-lift).
    { let _ = (0xCC00_0001u32); }
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
        Some(StyleWhiteSpace::Pre | StyleWhiteSpace::PreWrap |
StyleWhiteSpace::PreLine)
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
                    if !s.chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')) {
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
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/counter/fixed-point cast
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
    // Cache the env check in a `OnceLock<bool>`: this branch
    // fires once per dirty node (hundreds on cold layout),
    // and a direct `env::var` is a mutex + hashmap lookup
    // on macOS (~100 ns/call) even when the env var is unset.
    static FP_DUMP_ENABLED: std::sync::OnceLock<bool> =
        std::sync::OnceLock::new();
    let node_data = &styled_dom.node_data.as_container()[new_dom_id];

    let old_cold = old_tree.and_then(|t| old_tree_idx.and_then(|idx| t.cold(idx)));
    match (old_tree.is_some(), old_tree_idx.is_some(), old_cold.is_some()) {
        (false, _, _) => drop(crate::probe::Probe::span("recon_old_tree_none")),
        (true, false, _) => drop(crate::probe::Probe::span("recon_old_idx_none")),
        (true, true, false) => drop(crate::probe::Probe::span("recon_cold_none")),
        (true, true, true) => drop(crate::probe::Probe::span("recon_cold_some")),
    }

    // Compute the new multi-field fingerprint instead of a single hash.
    let new_fingerprint = {
        let _p = crate::probe::Probe::span("fingerprint_compute");
        NodeDataFingerprint::compute(
            node_data,
            styled_dom.styled_nodes.as_container().get(new_dom_id).map(|n| &n.styled_node_state),
        )
    };

    // Compare fingerprints to determine what changed (Layout, Paint, or Nothing).
    let dirty_flag = old_cold.map_or_else(|| {
            drop(crate::probe::Probe::span("fp_new_node"));
            DirtyFlag::Layout // new node → full layout
        }, |old_c| {
            let change_set = old_c.node_data_fingerprint.diff(&new_fingerprint);
            if change_set.needs_layout() {
                drop(crate::probe::Probe::span("fp_needs_layout"));
                let enabled = *FP_DUMP_ENABLED.get_or_init(|| {
                    std::env::var_os("AZ_FP_DUMP").is_some()
                });
                if enabled {
                    use std::sync::atomic::{AtomicUsize, Ordering};
                    static DUMPED: AtomicUsize = AtomicUsize::new(0);
                    let n = DUMPED.fetch_add(1, Ordering::Relaxed);
                    if n < 10 {
                        eprintln!(
                            "[fp_diff {n}] dom={} old={:?} new={:?}",
                            new_dom_id.index(),
                            old_c.node_data_fingerprint,
                            new_fingerprint,
                        );
                    }
                }
                DirtyFlag::Layout
            } else if change_set.needs_paint() {
                drop(crate::probe::Probe::span("fp_needs_paint"));
                DirtyFlag::Paint
            } else {
                drop(crate::probe::Probe::span("fp_clean"));
                DirtyFlag::None
            }
        });
    let is_dirty = dirty_flag >= DirtyFlag::Paint;

    // M12.7: `|| old_tree.is_none()` — on COLD layout there is no old tree to
    // clone, so we MUST create a fresh node; taking the else-branch would hit
    // `ok_or(InvalidTree)` on a None old_tree. This is both semantically correct
    // AND robust against a mis-lifted `dirty_flag`/Option match (the suspected
    // niche-enum mis-discriminant) wrongly steering cold nodes into the else.
    let new_node_idx = if dirty_flag >= DirtyFlag::Layout || old_tree.is_none() {
        { let _ = (0xBB00_0001u32); }
        let idx = new_tree_builder.create_node_from_dom(
            styled_dom,
            new_dom_id,
            new_parent_idx,
            debug_messages,
        );
        // Blockify replaced/inline flex-or-grid items (CSS Display 3 §2.7). The
        // full `process_node` build does this; this incremental path called
        // `create_node_from_dom` directly and skipped it, so a flex-item <img>
        // (e.g. the AzulPaint canvas) stayed inline and ignored flex-grow.
        new_tree_builder.blockify_node_display(styled_dom, new_dom_id, idx, new_parent_idx);
        idx
    } else {
        { let _ = (0xBB00_0002u32); }
        // Paint-only or clean: clone the old node (preserving layout cache)
        let old_full_node = old_tree
            .and_then(|t| old_tree_idx.and_then(|idx| t.get_full_node(idx)))
            .ok_or(LayoutError::InvalidTree)?;
        let mut idx = new_tree_builder.clone_node_from_old(&old_full_node, new_parent_idx);
        // If paint-only change, update the fingerprint and dirty flag
        if dirty_flag == DirtyFlag::Paint {
            if let Some(cloned) = new_tree_builder.get_mut(idx) {
                cloned.node_data_fingerprint = new_fingerprint;
                cloned.dirty_flag = DirtyFlag::Paint;
            }
        }
        idx
    };

    // reconcile_recursive sees it. 0 = correct (the first node); 64 (matching the
    // build-marker root_idx) = the usize return mis-reads here.
    { let _ = (0xAB00_0000u32 | (new_node_idx as u32 & 0xffff)); }

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
    let mut new_children_dom_ids: Vec<_> = collect_children_dom_ids(styled_dom, new_dom_id);

    // CSS 2.2 §17.2.1: Filter whitespace-only text nodes from table structural elements
    // (table, row-group, row). Without this, the reconciler sees them as "inline" children
    // mixed with block-level <td>/<th>, triggering incorrect anonymous IFC wrapping.
    // The layout tree builder already does this via should_skip_for_table_structure().
    {
        use super::getters::{get_display_property, MultiValue};
        let parent_display = match get_display_property(styled_dom, Some(new_dom_id)) {
            MultiValue::Exact(d) => d,
            _ => LayoutDisplay::Block,
        };
        if matches!(parent_display,
            LayoutDisplay::Table
            | LayoutDisplay::InlineTable
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
            | LayoutDisplay::TableRow
        ) {
            new_children_dom_ids.retain(|&id| {
                !super::layout_tree::is_whitespace_only_text(styled_dom, id)
            });
        }
    }

    // Compute both positional and DOM-keyed lookups for the old
    // tree's children. The DOM-keyed map is authoritative for
    // reconciliation (positional drifts every time the layout-tree
    // builder drops a DOM child — whitespace text, display:none,
    // table-structural whitespace — or inserts an anonymous
    // wrapper that isn't in the DOM).
    let old_children_indices: Vec<usize> = old_tree
        .and_then(|t| old_tree_idx.map(|idx| t.children(idx).to_vec()))
        .unwrap_or_default();
    let old_children_by_dom: alloc::collections::BTreeMap<NodeId, usize> = old_tree
        .and_then(|t| old_tree_idx.map(|idx| {
            t.children(idx).iter()
                .filter_map(|&cidx| t.get(cidx).and_then(|n| n.dom_node_id).map(|did| (did, cidx)))
                .collect()
        }))
        .unwrap_or_default();

    // Count of old layout children that correspond to a real DOM
    // node (exclude anonymous wrappers). This is what we compare
    // against the layout-relevant subset of new DOM children to
    // decide whether the structural shape actually changed.
    let old_layout_relevant_count = old_children_by_dom.len();

    // Filter new DOM children to the subset the layout-tree builder
    // would actually emit. This mirrors `should_skip_for_table_structure`
    // and the `is_whitespace_only_inline_run` logic. Without this
    // filter, `children_are_different` fires on every reconcile
    // because the DOM has whitespace text nodes the layout tree
    // drops.
    let new_layout_relevant_count = layout_relevant_child_count(styled_dom, &new_children_dom_ids, new_dom_id);

    let mut children_are_different = new_layout_relevant_count != old_layout_relevant_count;
    let mut new_child_hashes = Vec::new();

    // +spec:display-property:42f9c0 - anonymous block boxes wrap inline runs when block container has mixed block/inline children
    // CSS 2.2 Section 9.2.1.1: Anonymous Block Boxes
    // When a block container has mixed block/inline children, we must:
    // 1. Wrap consecutive inline children in anonymous block boxes
    // 2. Leave block-level children as direct children

    let has_block_child = new_children_dom_ids
        .iter()
        .any(|&id| is_block_level(styled_dom, id));

    // CSS Flexbox §4 / Grid §6: every in-flow child of a flex/grid container
    // becomes a (blockified) flex/grid item. Anonymous-block wrapping of inline
    // runs is a BLOCK-container concept and must NOT apply here — otherwise an
    // inline-level child (e.g. an <img> with flex-grow, default display
    // inline-block) gets wrapped in an anonymous IFC block, so it's no longer a
    // direct flex item and its flex-grow is ignored (laid out 300×0). Processing
    // each child directly lets `blockify_node_display` (in create_node_from_dom)
    // see the flex/grid parent and blockify the child into a real flex item.
    let parent_is_flex_or_grid = matches!(
        get_display_type(styled_dom, new_dom_id),
        LayoutDisplay::Flex
            | LayoutDisplay::InlineFlex
            | LayoutDisplay::Grid
            | LayoutDisplay::InlineGrid
    );

    if !has_block_child || parent_is_flex_or_grid {
        // All children are inline (block container) OR the parent is a flex/grid
        // container (all children are direct items) — no anonymous boxes needed.
        // Process each child directly.
        for (i, &new_child_dom_id) in new_children_dom_ids.iter().enumerate() {
            // DOM-ID match rather than positional — tree builder
            // may have dropped some DOM children (whitespace text
            // nodes) so positional drift mis-aligns the cache.
            // DOM-id match only: positional fallback would align
            // anonymous wrappers against real DOM nodes and trigger
            // spurious fingerprint mismatches (see fp_diff dump).
            let old_child_idx = old_children_by_dom.get(&new_child_dom_id).copied();

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

            if old_tree.and_then(|t| t.cold(old_child_idx?).map(|n| n.subtree_hash))
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
                    // +spec:display-property:bef3fc - anonymous blocks of only collapsible whitespace removed from rendering tree
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
                        // Inline children live under the anon wrapper
                        // in the old tree, so the parent's direct
                        // `old_children_by_dom` map won't hit them.
                        // Fall through to the global `dom_to_layout`
                        // map; we don't care which anon wrapper they
                        // were under, only that their cold data
                        // (fingerprint) gets matched correctly.
                        let old_child_idx = old_children_by_dom.get(&inline_dom_id).copied()
                            .or_else(|| old_tree
                                .and_then(|t| t.dom_to_layout.get(&inline_dom_id))
                                .and_then(|v| v.first().copied()));
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

                    // NOTE: We intentionally do NOT unconditionally
                    // mark the anonymous wrapper as intrinsic_dirty
                    // here. If any of the inline children are
                    // themselves dirty, their own `mark_dirty` call
                    // propagates upward through this wrapper, so
                    // wrappers whose content is unchanged keep their
                    // cached layout. Setting `children_are_different`
                    // when the wrapper is newly created (no matching
                    // old anon) flips the parent to layout-dirty,
                    // which is what triggers a fresh wrapper layout.
                    children_are_different = true;
                    } // end else (non-whitespace run)
                }

                // Process block-level child directly under parent
                let old_child_idx = old_children_by_dom.get(&new_child_dom_id).copied()
                    .or_else(|| old_children_indices.get(i).copied());
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

                if old_tree.and_then(|t| t.cold(old_child_idx?).map(|n| n.subtree_hash))
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
                let old_child_idx = old_children_by_dom.get(&inline_dom_id).copied();
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

            // See note in main mixed-content branch: rely on
            // children's own mark_dirty to propagate upward rather
            // than invalidating the whole wrapper each reconcile.
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
/// For anonymous boxes (no `dom_node_id`), we use default values and inherit
/// from the containing block.
fn prepare_layout_context<'a, T: ParsedFontTrait>(
    ctx: &LayoutContext<'a, T>,
    tree: &LayoutTree,
    node_index: usize,
    containing_block_size: LogicalSize,
) -> Result<PreparedLayoutContext<'a>> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
    let warm = tree.warm(node_index).ok_or(LayoutError::InvalidTree)?;
    let dom_id = node.dom_node_id; // Can be None for anonymous boxes

    // Phase 1: Calculate this node's provisional used size

    // This size is based on the node's CSS properties (width, height, etc.) and
    // its containing block. If height is 'auto', this is a temporary value.
    let intrinsic = warm.intrinsic_sizes.unwrap_or_default();
    let final_used_size = calculate_used_size_for_node(
        ctx.styled_dom,
        dom_id, // Now Option<NodeId>
        &containing_block_size,
        intrinsic,
        &node.box_props.unpack(),
        &ctx.viewport_size,
    )?;

    // Phase 2: Layout children using a formatting context
    // Use pre-computed styles from LayoutNodeWarm instead of repeated lookups
    let writing_mode = warm.computed_style.writing_mode;
    let text_align = warm.computed_style.text_align;
    let display = warm.computed_style.display;
    let overflow_y = warm.computed_style.overflow_y;

    // Check if height is auto (no explicit height set)
    let height_is_auto = warm.computed_style.height.is_none();

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

    let wm_ctx = crate::solver3::geometry::WritingModeContext::new(
        writing_mode,
        warm.computed_style.direction,
        warm.computed_style.text_orientation,
    );
    let constraints = LayoutConstraints {
        available_size: available_size_for_children,
        bfc_state: None,
        writing_mode,
        writing_mode_ctx: wm_ctx,
        text_align: style_text_align_to_fc(text_align),
        containing_block_size,
        available_width_type: Text3AvailableSpace::Definite(available_size_for_children.width),
    };

    Ok(PreparedLayoutContext {
        constraints,
        dom_id,
        writing_mode,
        final_used_size,
        box_props: node.box_props.unpack(),
    })
}

/// Core scrollbar info computation: given pre-computed content and container sizes plus
/// a DOM node for style look-up, determines whether scrollbars are needed.
///
/// This is the single source of truth for scrollbar detection. Both the BFC path
/// (`compute_scrollbar_info`) and the Taffy flex/grid path (`compute_child_layout`
/// in `taffy_bridge.rs`) call this function, ensuring consistent behaviour.
///
/// For paged media (PDF), scrollbars are never added since they don't exist in print.
pub fn compute_scrollbar_info_core<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    dom_id: NodeId,
    styled_node_state: &azul_core::styled_dom::StyledNodeState,
    content_size: LogicalSize,
    container_size: LogicalSize,
) -> ScrollbarRequirements {
    // +spec:overflow:08b60d - non-interactive media: UA may show scroll indicators but we skip them for print
    if ctx.fragmentation_context.is_some() {
        return ScrollbarRequirements::default();
    }

    let overflow_x = get_overflow_x(ctx.styled_dom, dom_id, styled_node_state);
    let overflow_y = get_overflow_y(ctx.styled_dom, dom_id, styled_node_state);

    // Resolve the full scrollbar style **once** and reuse it
    // across the rest of this function + any further calls from
    // the same layout pass via `LayoutContext::scrollbar_style_cache`.
    // Previously we called `get_layout_scrollbar_width_px` (which
    // builds the full scrollbar_style internally, keeps only
    // `reserve_width_px`, then drops it) and then
    // `get_scrollbar_style` again — each build performs 9 cascade
    // walks (track/thumb/button/corner/width/color/visibility/
    // fade-delay/fade-duration). With the memo, subsequent calls
    // on the same (dom_id, state) are a HashMap hit.
    let scrollbar_style = crate::solver3::getters::get_scrollbar_style_cached(
        ctx, dom_id, styled_node_state,
    );
    let scrollbar_width_px = scrollbar_style.reserve_width_px;

    let mut reqs = fc::check_scrollbar_necessity(
        content_size,
        container_size,
        to_overflow_behavior(overflow_x),
        to_overflow_behavior(overflow_y),
        scrollbar_width_px,
    );
    reqs.visual_width_px = scrollbar_style.visual_width_px;

    // +spec:overflow:e90f12 - scrollbar-gutter reserves space independently of scrollbar presence
    // +spec:overflow:3c44cc - scrollbar-gutter: stable reserves gutter even when no scrollbar is shown
    // +spec:overflow:3a6966 - classic scrollbar gutter width == scrollbar width; overlay scrollbars have no gutter
    //
    // scrollbar-gutter only applies to scroll containers (overflow: auto or scroll).
    // "stable" reserves gutter on the inline-end edge even if no scrollbar is needed.
    // "stable both-edges" reserves gutter on both inline edges.
    let scrollbar_gutter = get_scrollbar_gutter_property(ctx.styled_dom, dom_id, styled_node_state)
        .unwrap_or(azul_css::props::layout::overflow::StyleScrollbarGutter::Auto);
    let ob_y = to_overflow_behavior(overflow_y);
    let is_scroll_container = matches!(ob_y, fc::OverflowBehavior::Scroll | fc::OverflowBehavior::Auto);

    if is_scroll_container {
        use azul_css::props::layout::overflow::StyleScrollbarGutter;
        match scrollbar_gutter {
            StyleScrollbarGutter::Stable => {
                // Reserve gutter on inline-end even if no scrollbar is currently needed
                if !reqs.needs_vertical {
                    reqs.scrollbar_width = scrollbar_width_px;
                }
            }
            StyleScrollbarGutter::StableBothEdges => {
                // Reserve gutter on both inline edges
                reqs.scrollbar_width = scrollbar_width_px * 2.0;
            }
            StyleScrollbarGutter::Auto => {
                // Default: gutter only present when scrollbar is present (already handled)
            }
        }
    }

    reqs
}

/// Determines scrollbar requirements for a node based on content overflow.
///
/// Convenience wrapper around `compute_scrollbar_info_core` for the BFC layout path,
/// where the container size is derived from `box_props.inner_size(final_used_size, …)`.
fn compute_scrollbar_info<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    dom_id: NodeId,
    styled_node_state: &azul_core::styled_dom::StyledNodeState,
    content_size: LogicalSize,
    box_props: &crate::solver3::geometry::BoxProps,
    final_used_size: LogicalSize,
    writing_mode: LayoutWritingMode,
) -> ScrollbarRequirements {
    let container_size = box_props.inner_size(final_used_size, writing_mode);
    compute_scrollbar_info_core(ctx, dom_id, styled_node_state, content_size, container_size)
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

    let Some(warm_node) = tree.warm(node_index) else {
        return false;
    };

    warm_node.scrollbar_info.as_ref().map_or_else(|| scrollbar_info.needs_reflow(), |old_info| {
            // Trigger reflow if scrollbar state changed in either direction
            let horizontal_changed = old_info.needs_horizontal != scrollbar_info.needs_horizontal;
            let vertical_changed = old_info.needs_vertical != scrollbar_info.needs_vertical;
            horizontal_changed || vertical_changed
        })
}

/// Calculates the content-box position from a margin-box position.
///
/// The content-box is offset from the margin-box by border + padding.
/// Margin is NOT added here because `containing_block_pos` already accounts for it.
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
    current_node: &LayoutNodeHot,
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
        }).map_or_else(|| "Unknown".to_string(), |n| format!("{:?}", n.node_type));

    let cbp = current_node.box_props.unpack();
    debug_msgs.push(LayoutDebugMessage::new(
        LayoutDebugMessageType::PositionCalculation,
        format!(
            "[CONTENT BOX {}] {} - margin-box pos=({:.2}, {:.2}) + border=({:.2},{:.2}) + \
             padding=({:.2},{:.2}) = content-box pos=({:.2}, {:.2})",
            node_index,
            dom_name,
            containing_block_pos.x,
            containing_block_pos.y,
            cbp.border.left,
            cbp.border.top,
            cbp.padding.left,
            cbp.padding.top,
            self_content_box_pos.x,
            self_content_box_pos.y
        ),
    ));
}

/// Emits debug logging for child positioning if debug messages are enabled.
fn log_child_positioning<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    child_index: usize,
    child_node: &LayoutNodeHot,
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
        }).map_or_else(|| "Unknown".to_string(), |n| format!("{:?}", n.node_type));

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
            child_node.box_props.unpack().margin.left,
            child_node.box_props.unpack().margin.top,
            child_absolute_pos.x,
            child_absolute_pos.y
        ),
    ));
}

/// Processes a single in-flow child: sets position and recurses.
///
/// For Flex/Grid containers, Taffy has already laid out the children completely.
/// We only recurse to position their grandchildren.
/// For Block/Inline/Table, `layout_bfc/layout_ifc` already laid out children in Pass 1.
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
    float_cache: &mut HashMap<usize, fc::FloatingContext>,
) -> Result<()> {
    // Set relative position on child
    // child_relative_pos is [CoordinateSpace::Parent] - relative to parent's content-box
    let child_warm = tree.warm_mut(child_index).ok_or(LayoutError::InvalidTree)?;
    child_warm.relative_position = Some(child_relative_pos);

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
    let child_bp = child_node.box_props.unpack();
    let child_content_box_pos =
        calculate_content_box_pos(child_absolute_pos, &child_bp);
    let child_inner_size = child_bp
        .inner_size(child_node.used_size.unwrap_or_default(), writing_mode);
    let child_children: Vec<usize> = tree.children(child_index).to_vec();
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
/// The layout was already computed by `layout_bfc/layout_ifc`.
/// We only need to convert relative positions to absolute positions.
fn position_bfc_child_descendants(
    tree: &LayoutTree,
    node_index: usize,
    content_box_pos: LogicalPosition,
    calculated_positions: &mut super::PositionVec,
) {
    let Some(node) = tree.get(node_index) else { return };

    for &child_index in tree.children(node_index) {
        let Some(child_node) = tree.get(child_index) else { continue };

        // Use the relative_position that was set during formatting context layout
        let child_rel_pos = tree.warm(child_index)
            .and_then(|w| w.relative_position)
            .unwrap_or_default();
        let child_abs_pos = LogicalPosition::new(
            content_box_pos.x + child_rel_pos.x,
            content_box_pos.y + child_rel_pos.y,
        );

        super::pos_set(calculated_positions, child_index, child_abs_pos);

        // Calculate child's content-box position for recursion
        let cbp = child_node.box_props.unpack();
        let child_content_box_pos = LogicalPosition::new(
            child_abs_pos.x + cbp.border.left + cbp.padding.left,
            child_abs_pos.y + cbp.border.top + cbp.padding.top,
        );
        
        // Recurse to grandchildren
        position_bfc_child_descendants(tree, child_index, child_content_box_pos, calculated_positions);
    }
}

/// Processes out-of-flow children (absolute/fixed positioned elements).
///
/// Out-of-flow elements don't appear in `layout_output.positions` but still need
/// a static position for when no explicit offsets are specified. This sets their
/// static position to the parent's content-box origin.
fn process_out_of_flow_children<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    self_content_box_pos: LogicalPosition,
    containing_block_size: LogicalSize,
    calculated_positions: &mut super::PositionVec,
    reflow_needed_for_scrollbars: &mut bool,
    float_cache: &mut HashMap<usize, fc::FloatingContext>,
) -> Result<()> {
    // Collect out-of-flow children (those not already positioned)
    let out_of_flow_children: Vec<(usize, Option<NodeId>)> = {
        let current_node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        tree.children(node_index)
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

        // Perform full layout for the absolutely positioned child so its
        // inline_layout_result is populated (text rendering needs this).
        // The containing block for abs-pos is the parent's padding box.
        calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            child_index,
            self_content_box_pos,
            containing_block_size,
            calculated_positions,
            reflow_needed_for_scrollbars,
            float_cache,
            ComputeMode::PerformLayout,
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
/// When Pass 1 measures a node with `available_size` A and gets `result_size` R,
/// then Pass 2 provides R as a `known_dimension`, `get_size()` / `get_layout()`
/// recognize R == `cached.result_size` as a cache hit. This is the fundamental
/// mechanism ensuring O(n) total complexity across both passes.
#[allow(clippy::implicit_hasher)] // internal helper; only ever called with the default-hasher HashMap/HashSet
pub fn calculate_layout_for_subtree<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    containing_block_pos: LogicalPosition,
    containing_block_size: LogicalSize,
    calculated_positions: &mut super::PositionVec,
    reflow_needed_for_scrollbars: &mut bool,
    float_cache: &mut HashMap<usize, fc::FloatingContext>,
    compute_mode: ComputeMode,
) -> Result<()> {
    // [g147b az-web-lift DIAG] per-node calculate_layout_for_subtree entry (0x60980+slot): records the
    // last compute_mode that reached this node (PerformLayout=2 wins, runs after ComputeSize=1). If a div
    // shows 0x...0002 here but its layout_formatting_context marker (0x609A0+) is UNSET → positioning
    // reached calculate but short-circuited (cache hit) before dispatching to the formatting context.
    #[cfg(feature = "web_lift")]
    unsafe {
        let m = match compute_mode { ComputeMode::PerformLayout => 0xC0DE0002u32, _ => 0xC0DE0001u32 };
        crate::az_mark(((0x60980 + (node_index & 7) * 4)) as u32, (m) as u32);
    }
    let _probe = match compute_mode {
        ComputeMode::ComputeSize => crate::probe::Probe::span("size_node"),
        ComputeMode::PerformLayout => crate::probe::Probe::span("pos_node"),
    };
    // HIT path; 0x60 = reached cache-miss compute.) Distinguishes stub/not-entered vs an
    // early Err in the cache-check vs the compute path.
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
                // ComputeSize: check measurement slot first (Taffy's 9-slot scheme).
                // TODO(superplan): only slot 0 is ever read/written — the other 8
                // measurement slots are dead. To wire the full multi-slot scheme,
                // classify `containing_block_size` into (width_known, height_known,
                // width_type, height_type) and select the slot via
                // `NodeCache::slot_index(..)` here and at the matching `store_size`.
                let sizing_hit = ctx.cache_map.entries[node_index]
                    .get_size(0, containing_block_size)
                    .cloned();
                if let Some(cached_sizing) = sizing_hit {
                    // SIZING CACHE HIT — set used_size and return immediately.
                    // No child positioning needed in ComputeSize mode.
                    drop(crate::probe::Probe::span("size_cache_hit_sizing"));
                    if let Some(node) = tree.get_mut(node_index) {
                        node.used_size = Some(cached_sizing.result_size);
                    }
                    if let Some(warm) = tree.warm_mut(node_index) {
                        warm.escaped_top_margin = cached_sizing.escaped_top_margin;
                        warm.escaped_bottom_margin = cached_sizing.escaped_bottom_margin;
                        warm.baseline = cached_sizing.baseline;
                    }
                    return Ok(());
                }
                // Fall through to layout slot check
                let layout_hit = ctx.cache_map.entries[node_index]
                    .get_layout(containing_block_size)
                    .cloned();
                if let Some(cached_layout) = layout_hit {
                    // Layout slot hit in ComputeSize mode — extract size only
                    drop(crate::probe::Probe::span("size_cache_hit_layout"));
                    if let Some(node) = tree.get_mut(node_index) {
                        node.used_size = Some(cached_layout.result_size);
                    }
                    if let Some(warm) = tree.warm_mut(node_index) {
                        warm.overflow_content_size = Some(cached_layout.content_size);
                        warm.scrollbar_info = Some(cached_layout.scrollbar_info);
                    }
                    return Ok(());
                }
                // [g147c az-web-lift DIAG] ComputeSize cache MISS for this node (0x60A60+slot): the
                // compute path WILL run → layout_formatting_context should fire. If a div is sized by
                // Pass-1 (0x60A40 set) but this miss-flag is UNSET → calculate(child,ComputeSize) hit
                // the cache instead (so layout_formatting_context/layout_ifc were skipped).
                #[cfg(feature = "web_lift")]
                unsafe { crate::az_mark(((0x60A60 + (node_index & 7) * 4)) as u32, (0xC0DE0001) as u32); }
                drop(crate::probe::Probe::span("size_cache_miss"));
            }
            ComputeMode::PerformLayout => {
                // PerformLayout: check layout slot (the single "full layout" slot)
                let layout_hit = ctx.cache_map.entries[node_index]
                    .get_layout(containing_block_size)
                    .cloned();
                if let Some(cached_layout) = layout_hit {
                    drop(crate::probe::Probe::span("pos_cache_hit"));
                    // LAYOUT CACHE HIT — apply cached results with child positions
                    if let Some(node) = tree.get_mut(node_index) {
                        node.used_size = Some(cached_layout.result_size);
                    }
                    if let Some(warm) = tree.warm_mut(node_index) {
                        warm.overflow_content_size = Some(cached_layout.content_size);
                        warm.scrollbar_info = Some(cached_layout.scrollbar_info.clone());
                    }

                    let box_props = tree.get(node_index)
                        .map(|n| n.box_props.unpack())
                        .unwrap_or_default();
                    let writing_mode = tree
                        .warm(node_index)
                        .map(|w| w.computed_style.writing_mode)
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
                            writing_mode,
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
    if compute_mode == ComputeMode::PerformLayout {
        drop(crate::probe::Probe::span("pos_cache_miss"));
    }

    // returned Ok; 0x64 = layout_formatting_context returned Ok. Last value before the
    // Err pins the failing phase (fires per recursive node; bare body is shallow).
    // Phase 1: Prepare layout context (calculate used size, constraints)
    let PreparedLayoutContext {
        constraints,
        dom_id,
        writing_mode,
        mut final_used_size,
        box_props,
    } = {
        let _p = crate::probe::Probe::span("prepare_layout_context");
        prepare_layout_context(ctx, tree, node_index, containing_block_size)?
    };

    // Phase 1.5: Update used_size BEFORE calling layout_formatting_context.
    //
    // When a node is cloned from the old tree (clone_node_from_old), its used_size
    // retains the value from the previous layout pass. If the containing block changed
    // (e.g. viewport resize), the stale used_size would cause layout_bfc() to compute
    // an incorrect children_containing_block_size. By updating used_size here, we ensure
    // that layout_bfc reads the freshly resolved size from prepare_layout_context.
    {
        let is_table_cell = tree.get(node_index).is_some_and(|n| {
            matches!(n.formatting_context, FormattingContext::TableCell)
        });
        if !is_table_cell {
            if let Some(node) = tree.get_mut(node_index) {
                node.used_size = Some(final_used_size);
            }
        }
    }

    // Phase 2: Layout children using the formatting context
    let layout_result = {
        let _p = crate::probe::Probe::span("layout_formatting_context");
        layout_formatting_context(ctx, tree, text_cache, node_index, &constraints, float_cache)?
    };
    let content_size = layout_result.output.overflow_size;

    // If layout_formatting_context adjusted this node's used_size (e.g.
    // layout_flex_grid auto-applying box-sizing:border-box on the root),
    // propagate that back into final_used_size so Phase 3 (scrollbars),
    // Phase 4 (final write), and the self_content_box_pos calculation all
    // see the same border-box that the children were laid out inside.
    if let Some(adjusted) = tree.get(node_index).and_then(|n| n.used_size) {
        final_used_size = adjusted;
    }

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

    // +spec:overflow:44ef3b - scroll container detection: overflow scroll/auto makes box a scroll container
    // Check if this node is a scroll container (overflow: scroll/auto).
    // Scroll containers must NOT expand to fit content — their height is
    // determined by the containing block, and overflow is scrollable.
    //
    // Exception: if the containing block height is infinite (unconstrained),
    // we must still grow, since you can't scroll inside an infinitely tall box.
    let is_scroll_container = dom_id.is_some_and(|id| {
        let ov_x = get_overflow_x(ctx.styled_dom, id, &styled_node_state);
        let ov_y = get_overflow_y(ctx.styled_dom, id, &styled_node_state);
        matches!(ov_x, MultiValue::Exact(LayoutOverflow::Scroll | LayoutOverflow::Auto))
            || matches!(ov_y, MultiValue::Exact(LayoutOverflow::Scroll | LayoutOverflow::Auto))
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
            )?;
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

    let merged_scrollbar_info = scrollbar_info;
    let content_box_size = box_props.inner_size(final_used_size, writing_mode);
    let inner_size_after_scrollbars = merged_scrollbar_info.shrink_size(content_box_size);

    // Phase 4: Update this node's state
    let self_content_box_pos = {
        {
            let current_node = tree.get_mut(node_index).ok_or(LayoutError::InvalidTree)?;

            // Table cells get their size from the table layout algorithm, don't overwrite
            let is_table_cell = matches!(
                current_node.formatting_context,
                FormattingContext::TableCell
            );
            if !is_table_cell || current_node.used_size.is_none() {
                current_node.used_size = Some(final_used_size);
            }
        }

        // Update warm fields
        if let Some(warm) = tree.warm_mut(node_index) {
            warm.scrollbar_info = Some(merged_scrollbar_info.clone());
            // Store overflow content size for scroll frame calculation
            // +spec:overflow:f28d6a - hanging glyphs should be ink overflow, not scrollable overflow (not yet subtracted from content_size)
            warm.overflow_content_size = Some(content_size);
        }

        // self_content_box_pos is [CoordinateSpace::Window] - the absolute position of this node's content-box
        let current_node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        let current_bp = current_node.box_props.unpack();
        let pos = calculate_content_box_pos(containing_block_pos, &current_bp);
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
        inner_size_after_scrollbars,
        calculated_positions,
        reflow_needed_for_scrollbars,
        float_cache,
    )?;

    // === STORE RESULT IN PER-NODE CACHE (Taffy-inspired 9+1 slot cache) ===
    // Store both the full layout entry and a sizing measurement entry.
    // This enables O(n) two-pass BFC: Pass 1 populates cache, Pass 2 reads it.
    if node_index < ctx.cache_map.entries.len() {
        let warm_ref = tree.warm(node_index);
        let baseline = warm_ref.and_then(|n| n.baseline);
        let escaped_top = warm_ref.and_then(|n| n.escaped_top_margin);
        let escaped_bottom = warm_ref.and_then(|n| n.escaped_bottom_margin);

        // Store in the layout slot (PerformLayout result)
        ctx.cache_map.get_mut(node_index).store_layout(LayoutCacheEntry {
            available_size: containing_block_size,
            result_size: final_used_size,
            content_size,
            child_positions: child_positions_for_cache,
            escaped_top_margin: escaped_top,
            escaped_bottom_margin: escaped_bottom,
            scrollbar_info: merged_scrollbar_info,
        });

        // Also store in a measurement slot (slot 0: both dimensions known).
        // This enables the "result matches request" optimization (Taffy pattern):
        // when Pass 2 provides the same size as Pass 1 measured, it's a cache hit.
        // TODO(superplan): see the matching note at the `get_size(0, ..)` site —
        // slots 1-8 are unused; wire `NodeCache::slot_index(..)` to populate them.
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
/// `used_size` and `relative_position` set, but their GRANDCHILDREN don't have positions
/// in `calculated_positions` yet. This function traverses down the tree and positions
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
    float_cache: &mut HashMap<usize, fc::FloatingContext>,
) -> Result<()> {
    let children: Vec<usize> = tree.children(node_index).to_vec();

    for &child_index in &children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_rel_pos = tree.warm(child_index)
            .and_then(|w| w.relative_position)
            .unwrap_or_default();
        let child_abs_pos = LogicalPosition::new(
            content_box_pos.x + child_rel_pos.x,
            content_box_pos.y + child_rel_pos.y,
        );

        // Insert position
        super::pos_set(calculated_positions, child_index, child_abs_pos);

        // Get child's content box for recursion
        let cbp = child_node.box_props.unpack();
        let child_writing_mode = tree
            .warm(child_index)
            .map(|w| w.computed_style.writing_mode)
            .unwrap_or_default();
        let child_content_box = LogicalPosition::new(
            child_abs_pos.x
                + cbp.border.left
                + cbp.padding.left,
            child_abs_pos.y
                + cbp.border.top
                + cbp.padding.top,
        );
        let child_inner_size = cbp.inner_size(
            child_node.used_size.unwrap_or_default(),
            child_writing_mode,
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
            LayoutHeight::MinContent | LayoutHeight::MaxContent | LayoutHeight::FitContent(_) => {
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
/// `max(min_height`, `min(content_height`, `max_height`)).
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
) -> Result<LogicalSize> {
    let node_props = tree.get(node_index).ok_or(LayoutError::InvalidTree)?.box_props.unpack();
    let main_axis_padding_border =
        node_props.padding.main_sum(writing_mode) + node_props.border.main_sum(writing_mode);

    // CRITICAL: 'old_main_size' holds the size constrained by min-height/max-height from Phase 1
    let old_main_size = used_size.main(writing_mode);
    let new_main_size = content_size.main(writing_mode) + main_axis_padding_border;

    // Final size = max(min_height_constrained_size, content_size)
    // This ensures that min-height is respected even when content is smaller
    let final_main_size = old_main_size.max(new_main_size);

    used_size = used_size.with_main(writing_mode, final_main_size);

    Ok(used_size)
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
#[allow(clippy::implicit_hasher)] // internal helper; only ever called with the default-hasher HashMap/HashSet
pub fn compute_counters(
    styled_dom: &StyledDom,
    tree: &LayoutTree,
    counters: &mut HashMap<(usize, String), i32>,
) {
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
    counters: &mut HashMap<(usize, String), i32>,
    counter_stacks: &mut HashMap<String, Vec<i32>>,
    scope_stack: &mut Vec<Vec<String>>,
) {
    let Some(node) = tree.get(node_idx) else {
        return;
    };

    // Skip pseudo-elements (::marker, ::before, ::after) for counter processing
    // Pseudo-elements inherit counter values from their parent element
    // but don't participate in counter-reset or counter-increment themselves
    if tree.warm(node_idx).and_then(|w| w.pseudo_element.as_ref()).is_some() {
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
    let Some(dom_id) = node.dom_node_id else {
        // For anonymous boxes, just recurse to children
        for &child_idx in tree.children(node_idx) {
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

    // FAST PATH: almost no nodes declare counter-reset/counter-increment.
    // Single-bit check in compact cache lets us skip two cascade walks per node.
    let has_counter_css = node_state.is_normal()
        && cache.compact_cache.as_ref().is_none_or(|cc| cc.has_counter(dom_id.index()));

    // Process counter-reset (now properly typed)
    let counter_reset = if has_counter_css {
        cache
            .get_counter_reset(node_data, &dom_id, node_state)
            .and_then(|v| v.get_property())
    } else {
        None
    };

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
    let counter_inc = if has_counter_css {
        cache
            .get_counter_increment(node_data, &dom_id, node_state)
            .and_then(|v| v.get_property())
    } else {
        None
    };

    if let Some(counter_inc) = counter_inc {
        let counter_name_str = counter_inc.counter_name.as_str();
        if counter_name_str != "none" {
            let counter_name = counter_name_str.to_string();
            let inc_value = counter_inc.value;

            // Increment the counter in the current scope
            let stack = counter_stacks.entry(counter_name).or_default();
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
        let stack = counter_stacks.entry(counter_name).or_default();
        if stack.is_empty() {
            // Auto-initialize if counter doesn't exist
            stack.push(1);
        } else if let Some(current) = stack.last_mut() {
            *current += 1;
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
    for &child_idx in tree.children(node_idx) {
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
