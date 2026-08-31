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

use crate::solver3::layout_tree::LayoutNodeId;
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
        layout::{LayoutDisplay, LayoutHeight, LayoutOverflow, LayoutPosition, LayoutWritingMode},
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
            get_css_height, get_display_property, get_overflow_x, get_overflow_y,
            get_scrollbar_gutter_property, get_text_align, get_white_space_property,
            get_writing_mode, MultiValue,
        },
        layout_tree::{
            get_display_type, is_block_level, AnonymousBoxType, DirtyFlag, LayoutNode,
            LayoutNodeHot, LayoutTreeBuilder, SubtreeHash,
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
#[derive(Copy, Debug, Clone)]
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
    #[must_use]
    pub fn slot_index(
        width_known: bool,
        height_known: bool,
        width_type: AvailableWidthType,
        height_type: AvailableWidthType,
    ) -> usize {
        match (width_known, height_known) {
            (true, true) => 0,
            (true, false) => {
                if width_type == AvailableWidthType::MinContent {
                    2
                } else {
                    1
                }
            }
            (false, true) => {
                if height_type == AvailableWidthType::MinContent {
                    4
                } else {
                    3
                }
            }
            (false, false) => {
                let w = usize::from(width_type == AvailableWidthType::MinContent);
                let h = usize::from(height_type == AvailableWidthType::MinContent);
                5 + w * 2 + h
            }
        }
    }

    /// Classify a CONCRETE containing-block size into (measurement slot,
    /// canonicalized key). The call sites only have resolved `f32`s — the
    /// availability enums are gone by this depth — so "indefinite" is
    /// detected by value: non-finite or the ≥1e9 sentinel family.
    ///
    /// WHY THIS EXISTS (2026-08-08, measured on big.md): with everything
    /// collapsed onto slot 0, ~507 nodes were each sized TWICE per pass —
    /// the measure visit under an INDEFINITE height and the final visit
    /// under the resolved height — and the single slot ping-ponged between
    /// the two keys: 506 `h_qinf` + 508 `h_sinf` misses, a structural 100%
    /// miss rate on a pass where nothing changed. Separate slots per
    /// known/unknown combination make both visits hit from the second pass
    /// on; canonicalizing the indefinite axis to `f32::MAX` keeps the
    /// epsilon compare meaningful (`INFINITY - INFINITY` is NaN, which
    /// fails every `<` test and would turn a same-key lookup into a miss).
    #[must_use]
    pub fn classify_size_key(containing_block_size: LogicalSize) -> (usize, LogicalSize) {
        const INDEFINITE: f32 = 1.0e9;
        let w = containing_block_size.width;
        let h = containing_block_size.height;
        let w_known = w.is_finite() && w.abs() < INDEFINITE;
        let h_known = h.is_finite() && h.abs() < INDEFINITE;
        let slot = Self::slot_index(
            w_known,
            h_known,
            AvailableWidthType::Definite,
            AvailableWidthType::Definite,
        );
        let key = LogicalSize {
            width: if w_known { w } else { f32::MAX },
            height: if h_known { h } else { f32::MAX },
        };
        (slot, key)
    }

    /// Look up a sizing cache entry, implementing Taffy's "result matches request"
    /// optimization: if the caller provides the result size as a known dimension
    /// (common in Pass1→Pass2 transitions), it's still a cache hit.
    #[must_use]
    pub fn get_size(&self, slot: usize, known_dims: LogicalSize) -> Option<&SizingCacheEntry> {
        if let Some(entry) = self.measure_entries[slot].as_ref() {
            // Exact match on input constraints
            if (known_dims.width - entry.available_size.width).abs() < CACHE_SIZE_EPSILON
                && (known_dims.height - entry.available_size.height).abs() < CACHE_SIZE_EPSILON
            {
                return Some(entry);
            }
        }
        // "Result matches request" — if the caller provides the result size
        // as a known dimension, it's still a hit. This is the key optimization
        // that makes two-pass layout O(n): Pass 1 measures a node, Pass 2
        // provides the measured size as a constraint → automatic cache hit.
        // Valid only for a fully-definite query (both axes are real lengths),
        // and the matching entry may live in ANY slot — pass 1 stored its
        // result under the measure-constraint slot, not the definite one.
        if known_dims.width < 1.0e9 && known_dims.height < 1.0e9 {
            for entry in self.measure_entries.iter().flatten() {
                if (known_dims.width - entry.result_size.width).abs() < CACHE_SIZE_EPSILON
                    && (known_dims.height - entry.result_size.height).abs() < CACHE_SIZE_EPSILON
                {
                    return Some(entry);
                }
            }
        }
        None
    }

    /// Store a sizing result in the given slot.
    pub const fn store_size(&mut self, slot: usize, entry: SizingCacheEntry) {
        self.measure_entries[slot] = Some(entry);
        self.is_empty = false;
    }

    /// Look up the full layout cache entry.
    #[must_use]
    pub fn get_layout(&self, known_dims: LogicalSize) -> Option<&LayoutCacheEntry> {
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
    #[must_use]
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
// Independent per-pass state FLAGS, not a state machine to enum-ify: each
// bool is set by a different stage and read by a different consumer.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub struct LayoutCache {
    /// The fully laid-out tree from the previous frame. This is our primary cache.
    pub tree: Option<LayoutTree>,
    /// One-shot latch: the next `layout_document` call is a RESIZE-ONLY
    /// relayout of the SAME `StyledDom` object (set by the dll's
    /// `incremental_relayout_for_resize`, consumed unconditionally at entry).
    /// Skips reconcile + `cache_map` remap wholesale — see the Step-1 branch
    /// in `layout_document` for the contract and the dom-id sanity guard.
    pub resize_only_hint: bool,
    /// Census: did the LAST `layout_document` take the resize-only
    /// reconcile-skip branch? The external observable that distinguishes
    /// "skipped the walk" from "walked and found everything clean" (both
    /// produce identical pixels and identical reuse censuses).
    pub last_reconcile_was_skipped: bool,
    /// The last reconcile RAN and preserved the tree's structure exactly —
    /// zero fresh nodes, zero drops, indices stable. Content changes (a text
    /// edit reflowing its IFC) land here, and it is what lets the per-IFC
    /// display-list patch engage for them: unchanged nodes splice their
    /// items from the previous DL, only reflowed IFCs re-emit.
    pub last_reconcile_structure_preserved: bool,
    /// Whether the last display-list BUILD went through the per-IFC patch
    /// (splice + re-emit) rather than a full emission — the honest marker
    /// the `dl_text_patch` law reads.
    pub last_build_was_patched: bool,
    /// The PRECISE damage of the last patched build (logical px): old ∪ new
    /// bounds of every re-emitted or moved/resized node. A patched build may
    /// change the ITEM COUNT (the new text emits different runs), which
    /// makes the renderer's old-vs-new item diff bail to a full repaint —
    /// but the patch knows exactly what it touched, so the renderers prefer
    /// this when the diff gives up. `None` after a full emission.
    pub last_patch_damage: Option<Vec<LogicalRect>>,
    /// Monotonic count of display-list BUILDS, full or patched.
    ///
    /// A renderer
    /// remembers the value it last presented against, so
    /// [`Self::pending_patch_damage`] can hand it the damage of EVERY
    /// patched build since — not just the last one.
    pub build_seq: u64,
    /// `build_seq` of the last FULL emission (0 = none yet).
    ///
    /// A renderer that
    /// has not presented that build cannot use the patch log in its place:
    /// its own item diff against the list it last presented is the
    /// authority, and when that bails it repaints in full.
    pub last_full_build_seq: u64,
    /// `(build_seq, damage)` of the patched builds since the last full
    /// emission, oldest first.
    ///
    /// WHY A LOG: two patched builds can land between two presents — a
    /// callback's `set_css_property` patch followed, in the same pass, by
    /// the `RefreshDom` it returns (a structure-preserved relayout is a
    /// patched build too). Each build's damage is relative to the LAYOUT
    /// before it, so the second knows nothing about the rect the first
    /// vacated; when `last_patch_damage` was simply overwritten and the
    /// renderer's item diff bailed (the rebuild changed the item count),
    /// that rect was never repainted — the slider dragged a trail of
    /// thumbs. Cleared by every full emission (its item diff against the
    /// last presented list covers everything); bounded, and a renderer that
    /// fell behind the window repaints in full.
    pub patch_damage_log: Vec<(u64, Vec<LogicalRect>)>,
    /// The `DynamicSelectorContext` the cached display list was BUILT under.
    /// The structure-preserved patch arm requires the current context to be
    /// EQUAL: a cascade-external flip (viewport crossing an @media bound, a
    /// theme/OS change) restyles reused nodes without touching `NodeData` or
    /// `css_dirty`, so spliced items would serve the OLD styles (the
    /// blue-desktop-box-after-crossing-to-mobile bug).
    pub last_dynamic_context: Option<azul_css::dynamic_selector::DynamicSelectorContext>,
    /// `used_size` of every layout node as of the PREVIOUS pass — captured at
    /// the resize-skip branch (the pass overwrites `used_size` in the shared
    /// tree object). DL patching diffs these against the new sizes: a node
    /// whose size changed must re-emit its items (a translated background
    /// rect would be the wrong SIZE, not just the wrong place).
    pub previous_sizes: Vec<Option<LogicalSize>>,
    /// GRANULAR DIFF channel (task #15b, one-shot): per flattened `NodeId`,
    /// `true` = the pre-cascade DOM fingerprints proved this node AND every
    /// ancestor unchanged on BOTH tiers (structure + style). Reconcile may
    /// then reuse the old node's fingerprint after a cheap state-hash check
    /// instead of re-hashing node content — the diff feeds the later
    /// stages instead of being thrown away. Set by the dll's full-produce
    /// arm; consumed (taken) by `reconcile_and_invalidate`.
    pub dom_diff_clean: Option<Vec<bool>>,
    /// Census: fingerprint computations skipped via `dom_diff_clean` in the
    /// LAST reconcile — the external observable for the channel's tests.
    pub last_fingerprint_skips: usize,
    /// Presentation hint of the LAST pass, set ONLY when the DL patch fired:
    /// the dominant translation + exceptions. The CPU compositor turns it
    /// into a retained-pixmap blit + strip repaint (round 3). One-shot in
    /// spirit — the consumer must guard against re-application on the SAME
    /// display list (buffers-held retries re-run the present path).
    pub last_patch_move: Option<super::display_list::PatchMoveSummary>,
    /// Reconciliation census of the LAST pass: how many nodes were CLONED
    /// from the previous tree (warm shaped-text + intrinsic caches carried
    /// forward) vs built FRESH (no warm data). This pair is what makes cache
    /// reuse TESTABLE from outside solver3: `resize_relayout_bug.rs` asserts
    /// a same-DOM viewport resize reuses every node — the regression that
    /// motivated it (`old_tree = None` on any viewport change) re-shaped 917
    /// paragraphs and re-measured 1112 intrinsic widths per resize while
    /// every test still passed, because rebuilt-from-scratch produces the
    /// same pixels as reused, just ~130 ms slower.
    pub last_reconcile_reused: usize,
    /// See [`Self::last_reconcile_reused`].
    pub last_reconcile_fresh: usize,
    /// How many nodes the LAST pass actually recomputed intrinsic sizes for
    /// (the `intrinsic_dirty` set at the final `calculate_intrinsic_sizes`
    /// call). Same testability rationale as the reconcile census: the
    /// scrollbar-reflow loop used to mark EVERY node intrinsic-dirty
    /// (`(0..len).collect()`) — 75 ms re-measuring a whole document whose
    /// content had not changed — and no pixel test could see it, because
    /// recomputed intrinsics equal reused intrinsics.
    pub last_intrinsic_dirty: usize,
    /// The final, absolute positions of all nodes from the previous frame.
    pub calculated_positions: super::PositionVec,
    /// The viewport size from the last layout pass, used to detect resizes.
    pub viewport: Option<LogicalRect>,
    /// Stable scroll IDs computed from `node_data_hash` (layout index -> scroll ID)
    pub scroll_ids: HashMap<LayoutNodeId, u64>,
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
    /// The third key component (`u64`) is the GPU-key-population fingerprint
    /// (`GpuValueCache::dl_emission_fingerprint`): the emitted list is a
    /// function of which nodes carry transform/opacity keys, and diff-driven
    /// animation mints keys AFTER a layout pass — matching on (hash, viewport)
    /// alone served the pre-key list back and made animations invisible.
    /// The fourth (`u64`) is the scroll GEOMETRY fingerprint
    /// (`scroll_geometry_fingerprint`): scrollbar necessity/track layout and
    /// the VirtualView placeholder consume the ScrollManager snapshot, and a
    /// VirtualView's published virtual size arrives ONLY through it — matching
    /// without it served the pre-publication list (no scrollbar) forever. Live
    /// scroll OFFSETS are deliberately excluded: they are GPU-animated so that
    /// scrolling never re-emits the list.
    /// Key = (root subtree hash, viewport, GPU-key-population fingerprint,
    /// scroll-geometry fingerprint, DL-INPUT fingerprint, list). The fifth
    /// component covers the inputs `layout_document` takes besides the DOM
    /// that change the emitted list: caret visibility + locations, text
    /// selections, IME preedit (`dl_input_fingerprint`). Without it a
    /// relayout right after a click served the PRE-CARET list verbatim - the
    /// caret only appeared when a blink tick rebuilt (2026-08-31).
    pub cached_display_list: Option<(
        SubtreeHash,
        LogicalRect,
        u64,
        u64,
        u64,
        std::sync::Arc<super::display_list::DisplayList>,
    )>,
    /// Raw pointer of the `StyledDom` from the previous layout pass. When the
    /// same `&StyledDom` reference is passed again AND the viewport is unchanged,
    /// skip reconcile entirely and return the cached display list (saves ~0.8 ms).
    pub prev_dom_ptr: usize,
    pub prev_viewport: LogicalRect,
}

/// Approximate heap-byte breakdown of the solver3 `LayoutCache`.
#[derive(Copy, Debug, Clone, Default)]
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
    /// `GlyphInstance`s inside the cached DL's `Text` items — the offset
    /// COPIES of `glyph_runs` instances. Counter, not a byte total (the
    /// bytes are inside `cached_display_list_bytes`).
    pub cached_display_list_text_instances: usize,
    /// Item slots in the cached DL (each costs `size_of::<DisplayListItem>()`
    /// inline). Counter, not a byte total.
    pub cached_display_list_items: usize,
}

impl Solver3CacheMemoryReport {
    #[must_use]
    pub const fn total_bytes(&self) -> usize {
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

/// What a renderer that last presented against `build_seq == consumed` still
/// has to repaint from patched builds, per [`LayoutCache::pending_patch_damage`].
#[derive(Debug, Clone, PartialEq)]
pub enum PendingPatchDamage {
    /// Nothing was built since the renderer's last present.
    None,
    /// Every build since then was patched, the chain is complete, and this
    /// is the union of their damage.
    Rects(Vec<LogicalRect>),
    /// A FULL emission happened since the renderer's last present.
    ///
    /// The log
    /// cannot stand in for that: the renderer's item diff against the list
    /// it last presented is the authority, and when that bails it repaints
    /// in full.
    FullBuildSincePresent,
    /// The renderer fell further behind than the log keeps: the damage is
    /// not known, repaint in full.
    Unknown,
}

/// How many patched builds the log keeps. A renderer presents after nearly
/// every build, so this is a bound on a pathological run of layouts between
/// presents, not a working set.
pub const PATCH_DAMAGE_LOG_CAP: usize = 16;

impl LayoutCache {
    /// A FULL emission happened: nothing patched is pending any more.
    pub fn record_full_emission(&mut self) {
        self.build_seq = self.build_seq.wrapping_add(1);
        self.last_full_build_seq = self.build_seq;
        self.last_build_was_patched = false;
        self.last_patch_damage = None;
        // The mover blit belongs to the PATCHED list it was computed for; a
        // full emission invalidates it exactly like the patch rects. Callers
        // used to clear this themselves (regenerate_display_list_for_dom
        // does), but the Step-1.1 cache-hit and early-exit paths called only
        // this function and left a stale TranslateHint armed - neutralized
        // today by the Arc-pointer guard, but nothing pinned that.
        self.last_patch_move = None;
        self.patch_damage_log.clear();
    }

    /// Record a PATCHED build's damage: the latest goes to `last_patch_damage`
    /// (what the frame report and the debug traces read) AND onto the log a
    /// renderer drains through [`Self::pending_patch_damage`].
    pub fn record_patch_damage(&mut self, rects: Vec<LogicalRect>) {
        self.build_seq = self.build_seq.wrapping_add(1);
        self.patch_damage_log.push((self.build_seq, rects.clone()));
        if self.patch_damage_log.len() > PATCH_DAMAGE_LOG_CAP {
            let excess = self.patch_damage_log.len() - PATCH_DAMAGE_LOG_CAP;
            self.patch_damage_log.drain(..excess);
        }
        self.last_patch_damage = Some(rects);
    }

    /// The damage of every patched build after the one a renderer last
    /// presented against (`consumed` = the `build_seq` it saw then).
    #[must_use]
    pub fn pending_patch_damage(&self, consumed: u64) -> PendingPatchDamage {
        if self.build_seq == consumed {
            return PendingPatchDamage::None;
        }
        if self.last_full_build_seq > consumed {
            return PendingPatchDamage::FullBuildSincePresent;
        }
        // Every build since `consumed` was patched, and the log is
        // consecutive from the last full emission on — so the chain is
        // complete exactly when its first entry is still logged.
        match self.patch_damage_log.first() {
            Some((oldest, _)) if *oldest <= consumed.wrapping_add(1) => {}
            _ => return PendingPatchDamage::Unknown,
        }
        let mut rects = Vec::new();
        for (seq, r) in &self.patch_damage_log {
            if *seq > consumed {
                rects.extend_from_slice(r);
            }
        }
        PendingPatchDamage::Rects(rects)
    }

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
    #[must_use]
    pub fn memory_report(&self) -> Solver3CacheMemoryReport {
        let tree_report = self.tree.as_ref().map(LayoutTree::memory_report);
        let tree_bytes = tree_report
            .as_ref()
            .map_or(0, super::layout_tree::LayoutTreeMemoryReport::total_bytes);
        // cache_map: Vec<NodeCache>; NodeCache has 9 Option<SizingCacheEntry>
        // + 1 Option<LayoutCacheEntry>. Count filled layout entries' child_positions.
        let mut cache_map_bytes = self.cache_map.entries.capacity() * size_of::<NodeCache>();
        for e in &self.cache_map.entries {
            if let Some(le) = &e.layout_entry {
                cache_map_bytes +=
                    le.child_positions.capacity() * size_of::<(usize, LogicalPosition)>();
            }
        }
        let cached_dl = self
            .cached_display_list
            .as_ref()
            .map_or((0, 0, 0), |(_, _, _, _, _, dl)| dl.retained_bytes());
        Solver3CacheMemoryReport {
            tree_bytes,
            tree_report,
            calculated_positions_bytes: self.calculated_positions.len()
                * size_of::<LogicalPosition>(),
            previous_positions_bytes: self.previous_positions.len() * size_of::<LogicalPosition>(),
            scroll_ids_bytes: self.scroll_ids.len() * (size_of::<usize>() + size_of::<u64>()),
            scroll_id_to_node_id_bytes: self.scroll_id_to_node_id.len()
                * (size_of::<u64>() + size_of::<NodeId>()),
            counters_bytes: self
                .counters
                .iter()
                .map(|((_, name), _)| {
                    size_of::<(usize, String)>() + size_of::<i32>() + name.capacity()
                })
                .sum(),
            float_cache_bytes: self.float_cache.len() * 256, // conservative per-FC
            cache_map_bytes,
            // Real walk — this was a FLAT 2048 guess, which hid the DL's
            // per-Text-item glyph copies (20 B/painted glyph) entirely.
            cached_display_list_bytes: cached_dl.0,
            cached_display_list_text_instances: cached_dl.1,
            cached_display_list_items: cached_dl.2,
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
    /// Nodes CLONED from the previous tree, warm data (shaped text, intrinsic
    /// widths) and all. The observable half of cache reuse — see
    /// [`LayoutCache::last_reconcile_reused`] for why this is load-bearing.
    pub reused_nodes: usize,
    /// Nodes built fresh (`create_node_from_dom`): genuinely new or
    /// Layout-dirty, with NO warm data. On a same-DOM relayout (a pure
    /// resize) this being anything but 0 means warm caches were thrown away.
    pub fresh_nodes: usize,
    /// The INDICES of those fresh nodes — the display-list patch re-emits
    /// exactly these (their previous items describe content that no longer
    /// exists: an edited text run, a same-count replace), instead of the
    /// whole intrinsic-dirty ancestor chain whose items are unchanged.
    pub fresh_indices: BTreeSet<usize>,
    /// Fingerprint computations skipped because the pre-cascade DOM diff
    /// proved the node (and its ancestors) unchanged. See
    /// `LayoutCache::dom_diff_clean`.
    pub fingerprint_skips: usize,
}

impl ReconciliationResult {
    /// Checks if any layout or paint work is needed.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.intrinsic_dirty.is_empty()
            && self.layout_roots.is_empty()
            && self.paint_dirty.is_empty()
    }

    /// Returns true if full layout work is needed for at least one node.
    #[must_use]
    pub fn needs_layout(&self) -> bool {
        !self.intrinsic_dirty.is_empty() || !self.layout_roots.is_empty()
    }

    /// Returns true if only paint work is needed (no layout).
    #[must_use]
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
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
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
        if let Some(parent_idx) = tree.get(LayoutNodeId::new(root_idx)).and_then(|n| n.parent) {
            parents_to_reposition.insert(parent_idx);
        }
    }

    for parent_idx in parents_to_reposition {
        let Some(parent_node) = tree.get(LayoutNodeId::new(parent_idx)) else {
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
pub(crate) fn to_overflow_behavior(overflow: MultiValue<LayoutOverflow>) -> fc::OverflowBehavior {
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
#[must_use]
pub fn collect_children_dom_ids(styled_dom: &StyledDom, parent_dom_id: NodeId) -> Vec<NodeId> {
    // The same child list the tree builder lays out (`layout_children`:
    // DOM children, minus inline transient windows grafted elsewhere, plus
    // the ones grafted onto this parent) - so a panel dropped onto another
    // zone reads as a STRUCTURAL change here and its old position is not
    // kept. Then the display:none filter.
    // +spec:display-property:9f02c6 - display:none elements generate no boxes
    // +spec:display-property:3b507e - display:none excludes subtree from box tree
    let children: Vec<NodeId> =
        crate::solver3::layout_tree::layout_children(styled_dom, parent_dom_id)
            .into_iter()
            .filter(|&child_id| get_display_type(styled_dom, child_id) != LayoutDisplay::None)
            .collect();

    // DEBUG (2026-06-02 children-None): record collected child count per parent
    // @0x40540+parent*4 (0xCC00_00NN). N=0 with first_child Some ⇒ get_display_type
    // mis-lift skipped them; N>0 ⇒ walk works. REVERT before commit.
    unsafe {
        let pi = parent_dom_id.index();
        if pi < 8 {
            crate::az_mark(
                (0x40540 + pi * 4) as u32,
                (0xCC00_0000u32 | (children.len() as u32 & 0xffff)),
            );
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
    let Some(parent_node) = tree.get(LayoutNodeId::new(parent_idx)) else {
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
        let Some(child_node) = tree.get(LayoutNodeId::new(child_idx)) else {
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
            let old_relative_pos = tree
                .warm(LayoutNodeId::new(child_idx))
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

    if let Some(node) = tree.get(LayoutNodeId::new(node_idx)) {
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
    // Table-structural parents drop whitespace per CSS 2.2 section 17.2.1;
    // flex/grid containers drop it per css-flexbox-1 section 4 /
    // css-grid-1 section 6 (whitespace-only anonymous items are not
    // rendered).
    let is_table_structural = matches!(
        parent_display,
        LayoutDisplay::Table
            | LayoutDisplay::InlineTable
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
            | LayoutDisplay::TableRow
            | LayoutDisplay::Flex
            | LayoutDisplay::InlineFlex
            | LayoutDisplay::Grid
            | LayoutDisplay::InlineGrid
    );

    let has_any_block_child = children.iter().any(|&id| is_block_level(styled_dom, id));

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

/// # Errors
///
/// Returns a `LayoutError` if layout reconciliation fails.
pub fn reconcile_and_invalidate<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    cache: &LayoutCache,
    viewport: LogicalRect,
    // GRANULAR DIFF (see LayoutCache::dom_diff_clean) — taken by the
    // caller (this fn only has &cache) and moved in.
    dom_diff_clean: Option<Vec<bool>>,
) -> Result<(LayoutTree, ReconciliationResult)> {
    let _probe_outer = crate::probe::Probe::span("reconcile_and_invalidate");
    let mut new_tree_builder = LayoutTreeBuilder::new(ctx.viewport_size);
    let mut recon_result = ReconciliationResult::default();
    // A viewport SIZE change invalidates every VIEWPORT-DEPENDENT computed
    // size — and nothing else. The old code dropped the ENTIRE cached tree
    // here (`old_tree = None`), which made every node reconcile as brand-new
    // (`recon_old_tree_none` 1209/1209 on a same-DOM resize) and threw away
    // every warm handle with it: measured on big.md, ~54 ms of re-SHAPING
    // (917 `text_shape_stage` calls — shaping does not depend on the
    // viewport) plus ~75 ms of intrinsic-width recomputation (also
    // viewport-independent) per resize, ~130 ms of a 246 ms pass re-deriving
    // bit-identical results. That was the single largest reason a drag-resize
    // could not approach interactive rates (scripts/RSS_MAP §36,
    // ICON_CACHE_AND_RELAYOUT_REUSE §4).
    //
    // KEEPING the tree is sound because the things that DO depend on the
    // viewport re-derive through keys, not through tree identity:
    //
    //   * size/layout cache slots are KEYED by `containing_block_size`
    //     (`NodeCache::get_size/get_layout`) — the new viewport enters as the
    //     root containing block and every affected chain misses its key and
    //     recomputes. This is exactly the mechanism that fixed #9 "grey on
    //     resize" (an abs-positioned node's containing block IS the
    //     viewport, so its key changes); dropping the whole tree on top of
    //     it was belt-and-braces from before the keys existed.
    //   * conditional (@media-style) properties evaluate per pass against
    //     the dynamic-selector context (`style_cache` is rebuilt per
    //     `LayoutContext`), and shaping keys carry the RESOLVED font size —
    //     a viewport-relative font re-shapes via its changed content hash.
    //   * `layout_roots.insert(0)` below still forces the full top-down
    //     layout pass at the new size; reconciliation merely decides what
    //     that pass may REUSE (shaped runs, intrinsic widths), not whether
    //     it runs.
    let viewport_resized = cache.viewport.is_none_or(|v| v.size != viewport.size);
    let old_tree = cache.tree.as_ref();

    if viewport_resized {
        recon_result.layout_roots.insert(0); // Root is always index 0
    }

    let root_dom_id = ctx
        .styled_dom
        .root
        .into_crate_internal()
        .unwrap_or(NodeId::ZERO);
    let clean_slice: Option<&[bool]> = dom_diff_clean
        .as_deref()
        .filter(|c| c.len() == ctx.styled_dom.node_data.as_ref().len());
    let root_idx = reconcile_recursive(
        ctx.styled_dom,
        root_dom_id,
        old_tree.map(|t| t.root),
        None,
        old_tree,
        &mut new_tree_builder,
        &mut recon_result,
        ctx.debug_messages,
        false, // the root has no ancestor whose restyle could reach it
        clean_slice,
    )?;

    // A dirty FLEX / GRID ITEM cannot be laid out on its own: its size is
    // decided by its container's flex algorithm, and its siblings' slots
    // move with it. Promote such a root to its container (walking up through
    // nested flex/grid containers). Without this the item was re-solved as
    // a standalone root with its own `min-height` while the container kept
    // the slot it had reserved from the item's content size — the TextArea
    // painted 64 px into a 36 px slot, over the widget beneath it. (The
    // `reposition_clean_subtrees` comment always claimed the parent would
    // "already be a layout root"; now it is.)
    let promoted_layout_roots: BTreeSet<usize> = recon_result
        .layout_roots
        .iter()
        .map(|&idx| {
            let mut root = idx;
            while let Some(parent) = new_tree_builder.get(root).and_then(|n| n.parent) {
                let parent_is_flex_or_grid = new_tree_builder.get(parent).is_some_and(|p| {
                    matches!(
                        p.formatting_context,
                        FormattingContext::Flex | FormattingContext::Grid
                    )
                });
                if !parent_is_flex_or_grid {
                    break;
                }
                root = parent;
            }
            root
        })
        .collect();
    recon_result.layout_roots = promoted_layout_roots;

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

    new_tree_builder.apply_split_previews(ctx.content_overlay, ctx.styled_dom);
    let new_tree = new_tree_builder.build(root_idx);
    // layout_document's step marker is stuck at 1 (post-`?` not reached), the
    // lifted `?` mis-discriminated this Ok as Err (niche-Result mis-lift).
    {
        let _ = (0xCC00_0001u32);
    }
    assert_dom_ids_are_in_range(&new_tree, ctx.styled_dom);
    Ok((new_tree, recon_result))
}

/// Every `dom_node_id` in a freshly reconciled tree must address a node of
/// the `StyledDom` it was reconciled against.
///
/// WHY THIS IS AN ASSERTION AND NOT A `Result`. A tree that survives with a
/// stale id does not fail — it succeeds at describing the WRONG node. Some
/// thirty display-list sites, the style getters, the hit-test areas and the
/// damage attribution all read `styled_dom` through this id, and each one
/// silently returns another node's data. The reported symptom was a ribbon
/// tab click repainting rects belonging to unrelated nodes; the only reason
/// it was ever caught is that `compute_counters` happened to index far
/// enough out of range to trip a bounds check. Checking the invariant once,
/// here, converts that whole class of silent corruption into one failure
/// that names the node.
///
/// Cost: one comparison per layout node, against a pass that does real work
/// per node.
fn assert_dom_ids_are_in_range(tree: &LayoutTree, styled_dom: &StyledDom) {
    let dom_len = styled_dom.node_data.as_container().internal.len();
    for idx in 0..tree.nodes.len() {
        let Some(dom_id) = tree.get(LayoutNodeId::new(idx)).and_then(|n| n.dom_node_id) else {
            continue;
        };
        assert!(
            dom_id.index() < dom_len,
            "layout node {idx} claims DOM node {} but the StyledDom it was \
             reconciled against has only {dom_len} nodes. A layout node's \
             identity must come from the DOM node it was reconciled AGAINST, \
             never from one it merely reused measurements from (see \
             `clone_node_from_old`).",
            dom_id.index(),
        );
    }
    for dom_id in tree.dom_to_layout.keys() {
        assert!(
            dom_id.index() < dom_len,
            "dom_to_layout maps DOM node {} but the StyledDom has only \
             {dom_len} nodes - a lookup through this map would hand out a \
             layout index for a node that does not exist",
            dom_id.index(),
        );
    }
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
        Some(StyleWhiteSpace::Pre | StyleWhiteSpace::PreWrap | StyleWhiteSpace::PreLine)
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
                    if !s
                        .chars()
                        .all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C'))
                    {
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

/// Ordinal-match a freshly created anonymous inline wrapper to the old
/// parent's Nth anon wrapper and, when the run's children are IDENTICAL
/// (same dom ids, same order), carry the warm caches forward.
///
/// Anonymous wrappers have no `dom_node_id`, so ordinary reconciliation
/// cannot see them — every reconcile pass recreated them cold and flipped
/// their parent to `children_are_different = true` UNCONDITIONALLY, which put
/// every paragraph's parent into `intrinsic_dirty` + `layout_roots` on EVERY
/// pass (measured on big.md: 240 intrinsic IFC roots and 112 inline-content
/// re-collections per resize with a bit-identical DOM, ~12 ms). The ordinal
/// IS the identity: wrappers exist only for inline runs, runs are ordered by
/// child position, so the Nth wrapper under a node corresponds to the Nth run
/// — the same matching `layout_document`'s `cache_map` remap already performs
/// post-hoc for the size caches.
///
/// Only SELF-VALIDATING or content-derived caches are carried:
/// `inline_content_cache` re-validates itself against the subtree
/// fingerprint, and `intrinsic_sizes` are content-derived with the children
/// verified identical. Layout-derived state (`inline_layout_result`, used
/// sizes, baselines) stays `None` and re-derives through the CB-size-keyed
/// caches like any other clean node.
///
/// Returns whether the wrapper matched; the caller folds `!matched` into
/// `children_are_different` instead of flipping it unconditionally. A DIRTY
/// child inside a matched run still invalidates through that child's own
/// `mark_dirty` propagation — matching the wrapper never masks content edits.
fn try_reuse_anon_wrapper(
    old_tree: Option<&LayoutTree>,
    old_parent_idx: Option<usize>,
    anon_ordinal: usize,
    inline_run: &[(usize, NodeId)],
    new_tree_builder: &mut LayoutTreeBuilder,
    anon_idx: usize,
) -> bool {
    let (Some(t), Some(op)) = (old_tree, old_parent_idx) else {
        return false;
    };
    // Children live on the TREE (hot/warm/cold split), not the node struct.
    let Some(old_anon) = t
        .children(op)
        .iter()
        .copied()
        .filter(|&c| {
            t.cold(LayoutNodeId::new(c))
                .is_some_and(|cold| cold.anonymous_type == Some(AnonymousBoxType::InlineWrapper))
        })
        .nth(anon_ordinal)
    else {
        return false;
    };
    let old_anon_children = t.children(old_anon);
    if old_anon_children.len() != inline_run.len() {
        return false;
    }
    let ids_match = old_anon_children
        .iter()
        .zip(inline_run.iter())
        .all(|(&oc, &(_, nid))| {
            t.get(LayoutNodeId::new(oc)).and_then(|n| n.dom_node_id) == Some(nid)
        });
    if !ids_match {
        return false;
    }
    if let (Some(old_warm), Some(new_node)) = (
        t.warm(LayoutNodeId::new(old_anon)),
        new_tree_builder.get_mut(anon_idx),
    ) {
        new_node
            .inline_content_cache
            .clone_from(&old_warm.inline_content_cache);
        new_node.intrinsic_sizes = old_warm.intrinsic_sizes;
    }
    true
}

/// Recursively traverses the new DOM and old tree, building a new tree and marking dirty nodes.
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/counter/fixed-point cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if recursive reconciliation fails.
pub fn reconcile_recursive(
    styled_dom: &StyledDom,
    new_dom_id: NodeId,
    old_tree_idx: Option<usize>,
    new_parent_idx: Option<usize>,
    old_tree: Option<&LayoutTree>,
    new_tree_builder: &mut LayoutTreeBuilder,
    recon: &mut ReconciliationResult,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ancestor_style_changed: bool,
    // GRANULAR DIFF: per-NodeId "self+ancestors unchanged on both
    // pre-cascade tiers". None = no diff available (full fingerprinting).
    dom_diff_clean: Option<&[bool]>,
) -> Result<usize> {
    // Cache the env check in a `OnceLock<bool>`: this branch
    // fires once per dirty node (hundreds on cold layout),
    // and a direct `env::var` is a mutex + hashmap lookup
    // on macOS (~100 ns/call) even when the env var is unset.
    static FP_DUMP_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let node_data = &styled_dom.node_data.as_container()[new_dom_id];

    let old_cold =
        old_tree.and_then(|t| old_tree_idx.and_then(|idx| t.cold(LayoutNodeId::new(idx))));
    match (
        old_tree.is_some(),
        old_tree_idx.is_some(),
        old_cold.is_some(),
    ) {
        (false, _, _) => drop(crate::probe::Probe::span("recon_old_tree_none")),
        (true, false, _) => drop(crate::probe::Probe::span("recon_old_idx_none")),
        (true, true, false) => drop(crate::probe::Probe::span("recon_cold_none")),
        (true, true, true) => drop(crate::probe::Probe::span("recon_cold_some")),
    }

    // Compute the new multi-field fingerprint instead of a single hash —
    // UNLESS the pre-cascade DOM diff proved this node (and its ancestors)
    // unchanged on both tiers. The diff cannot see runtime STATE
    // (hover/focus applied after produce), so a skip still requires the
    // state hash to match the old fingerprint; content/inline/attr hashing
    // (the expensive part) is what gets skipped.
    let diff_clean_here = dom_diff_clean
        .and_then(|c| c.get(new_dom_id.index()))
        .copied()
        .unwrap_or(false);
    let new_fingerprint = {
        let mut reused: Option<NodeDataFingerprint> = None;
        if diff_clean_here {
            if let Some(old_c) = old_cold {
                let state_hash = {
                    use core::hash::{Hash, Hasher};
                    let mut h = DefaultHasher::new();
                    if let Some(st) = styled_dom
                        .styled_nodes
                        .as_container()
                        .get(new_dom_id)
                        .map(|n| &n.styled_node_state)
                    {
                        st.hash(&mut h);
                    }
                    h.finish()
                };
                if state_hash == old_c.node_data_fingerprint.state_hash {
                    recon.fingerprint_skips += 1;
                    drop(crate::probe::Probe::span("fingerprint_skipped_by_diff"));
                    reused = Some(old_c.node_data_fingerprint);
                }
            }
        }
        if let Some(fp) = reused {
            fp
        } else {
            let _p = crate::probe::Probe::span("fingerprint_compute");
            NodeDataFingerprint::compute(
                node_data,
                styled_dom
                    .styled_nodes
                    .as_container()
                    .get(new_dom_id)
                    .map(|n| &n.styled_node_state),
            )
        }
    };

    // Compare fingerprints to determine what changed (Layout, Paint, or Nothing).
    let dirty_flag = old_cold.map_or_else(
        || {
            drop(crate::probe::Probe::span("fp_new_node"));
            DirtyFlag::Layout // new node → full layout
        },
        |old_c| {
            let change_set = old_c.node_data_fingerprint.diff(&new_fingerprint);
            if change_set.needs_layout() {
                drop(crate::probe::Probe::span("fp_needs_layout"));
                let enabled =
                    *FP_DUMP_ENABLED.get_or_init(|| std::env::var_os("AZ_FP_DUMP").is_some());
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
        },
    );
    // INHERITANCE. `NodeDataFingerprint` describes ONE node: content,
    // state, inline CSS, classes, callbacks, attributes. Nothing in it
    // mentions the parent - so a child whose own data is untouched compares
    // CLEAN even when the style it INHERITS changed underneath it, and gets
    // cloned complete with the `inline_layout_result` that has the old
    // computed colour shaped into its glyphs. `layout_ifc` is then never
    // re-entered for that node, so the IFC content hash - which DOES
    // include colour - never gets a chance to reject the stale layout, and
    // the display-list builder paints straight out of it.
    //
    // Traced on the ribbon: clicking tab 1 makes tab 0's header
    // Layout-dirty (its classes and inline props both change) but leaves
    // its child text node at DirtyFlag::None, and the deactivated tab's
    // label keeps painting Word-blue #2B579A instead of #444444.
    //
    // A node re-cascades its descendants when the parts of its fingerprint
    // that can change a COMPUTED INHERITED value move: inline CSS (declares
    // them), ids/classes (select the rules that declare them), and the
    // styled state (:hover/:focus/:active variants). Content, callbacks and
    // attributes cannot, so a plain text edit still invalidates only its
    // own node and the incremental-edit path is untouched.
    let own_style_changed = old_cold.is_none_or(|old_c| {
        let old_fp = &old_c.node_data_fingerprint;
        old_fp.inline_css_hash != new_fingerprint.inline_css_hash
            || old_fp.ids_classes_hash != new_fingerprint.ids_classes_hash
            || old_fp.state_hash != new_fingerprint.state_hash
    });
    let subtree_style_changed = ancestor_style_changed || own_style_changed;

    // An ancestor's restyle forces a REBUILD rather than a clone: the clone
    // is what carries the stale inline layout, so marking it paint-dirty
    // would repaint the wrong colour rather than re-resolve it.
    //
    // Only TEXT nodes need it. The staleness lives in shaped glyphs, which
    // only text produces; every other node re-reads its own computed style
    // from the cascade when the display list is built, and the cascade is
    // already correct (verified: the deactivated tab's DIV painted the right
    // background while its text child painted the old colour). Restricting
    // the rebuild to text nodes keeps the cost off the rest of the tree.
    // Measured (layout/tests/frame_perf.rs, release), baseline / unrestricted
    // / text-only:
    //   idle    19.35 / 20.11 / 19.25 ms
    //   resize  33.59 / 35.19 / 33.01 ms
    //   cold   140.97 / 148.01 / 137.12 ms
    // i.e. the unrestricted form cost 4-5%; restricted to text it is flat.
    let dirty_flag =
        if ancestor_style_changed && matches!(node_data.get_node_type(), NodeType::Text(_)) {
            DirtyFlag::Layout
        } else {
            dirty_flag
        };

    let is_dirty = dirty_flag >= DirtyFlag::Paint;

    // M12.7: `|| old_tree.is_none()` — on COLD layout there is no old tree to
    // clone, so we MUST create a fresh node; taking the else-branch would hit
    // `ok_or(InvalidTree)` on a None old_tree. This is both semantically correct
    // AND robust against a mis-lifted `dirty_flag`/Option match (the suspected
    // niche-enum mis-discriminant) wrongly steering cold nodes into the else.
    let new_node_idx = if dirty_flag >= DirtyFlag::Layout || old_tree.is_none() {
        {
            let _ = (0xBB00_0001u32);
        }
        recon.fresh_nodes += 1;
        let idx = new_tree_builder.create_node_from_dom(
            styled_dom,
            new_dom_id,
            new_parent_idx,
            debug_messages,
        );
        recon.fresh_indices.insert(idx);
        // Blockify replaced/inline flex-or-grid items (CSS Display 3 §2.7). The
        // full `process_node` build does this; this incremental path called
        // `create_node_from_dom` directly and skipped it, so a flex-item <img>
        // (e.g. the AzulPaint canvas) stayed inline and ignored flex-grow.
        new_tree_builder.blockify_node_display(styled_dom, new_dom_id, idx, new_parent_idx);
        idx
    } else {
        {
            let _ = (0xBB00_0002u32);
        }
        recon.reused_nodes += 1;
        // Paint-only or clean: clone the old node (preserving layout cache)
        let old_full_node = old_tree
            .and_then(|t| old_tree_idx.and_then(|idx| t.get_full_node(idx)))
            .ok_or(LayoutError::InvalidTree)?;
        // The clone reuses the OLD node's measurements under the NEW node's
        // identity: `old_tree_idx` may have come from the positional
        // fallback below, in which case `old_full_node.dom_node_id` names a
        // different (or, after the DOM shrank, a nonexistent) node.
        let mut idx =
            new_tree_builder.clone_node_from_old(&old_full_node, new_parent_idx, Some(new_dom_id));
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
    {
        let _ = (0xAB00_0000u32 | (new_node_idx as u32 & 0xffff));
    }

    // CRITICAL: For list-items, create a ::marker pseudo-element as the first child
    // This must be done after the node is created but before processing children
    // Per CSS Lists Module Level 3, ::marker is generated as the first child of list-items
    {
        use crate::solver3::getters::get_display_property;
        let display = get_display_property(styled_dom, Some(new_dom_id)).exact();

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
        if matches!(
            parent_display,
            LayoutDisplay::Table
                | LayoutDisplay::InlineTable
                | LayoutDisplay::TableRowGroup
                | LayoutDisplay::TableHeaderGroup
                | LayoutDisplay::TableFooterGroup
                | LayoutDisplay::TableRow
        ) {
            new_children_dom_ids
                .retain(|&id| !super::layout_tree::is_whitespace_only_text(styled_dom, id));
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
    let old_children_by_dom: BTreeMap<NodeId, usize> = old_tree
        .and_then(|t| old_tree_idx.map(|idx| {
            t.children(idx).iter()
                // Pseudo-element nodes (::marker on list items) are
                // layout-SYNTHESIZED children that carry their originating
                // node's dom id without being DOM children. Counting them made
                // every <li> compare old=2 vs new=1 and flip
                // `children_are_different` on every reconcile (360 clean
                // parents re-dirtied per resize on big.md — the whole list
                // content re-measured each pass); keying them into the by-dom
                // map could also alias a marker as its own host during lookup.
                .filter(|&&cidx| t.warm(LayoutNodeId::new(cidx)).is_none_or(|w| w.pseudo_element.is_none()))
                .filter_map(|&cidx| t.get(LayoutNodeId::new(cidx)).and_then(|n| n.dom_node_id).map(|did| (did, cidx)))
                .collect()
        }))
        .unwrap_or_default();

    // Count of old layout children that correspond to a real DOM node. The
    // old tree hides inline children UNDER anonymous wrappers, so the direct
    // children alone under-count every mixed-content parent (a wrapper is one
    // layout child holding N dom children) — the initializer then compared
    // old=1 against new=2 for a plain [text, div] parent and flipped
    // `children_are_different` on EVERY reconcile. That wrongness was masked
    // for as long as the wrapper sites flipped the flag unconditionally
    // anyway; ordinal-matched wrapper reuse unmasked it. Flatten wrappers:
    // count their dom-id'd children as if they were direct.
    let old_layout_relevant_count = old_children_by_dom.len()
        + old_tree.zip(old_tree_idx).map_or(0, |(t, oidx)| {
            t.children(oidx)
                .iter()
                .copied()
                .filter(|&c| {
                    t.cold(LayoutNodeId::new(c)).is_some_and(|cold| {
                        cold.anonymous_type == Some(AnonymousBoxType::InlineWrapper)
                    })
                })
                .map(|w| {
                    t.children(w)
                        .iter()
                        .filter(|&&cc| {
                            t.get(LayoutNodeId::new(cc))
                                .and_then(|n| n.dom_node_id)
                                .is_some()
                        })
                        .count()
                })
                .sum::<usize>()
        });

    // Filter new DOM children to the subset the layout-tree builder
    // would actually emit. This mirrors `should_skip_for_table_structure`
    // and the `is_whitespace_only_inline_run` logic. Without this
    // filter, `children_are_different` fires on every reconcile
    // because the DOM has whitespace text nodes the layout tree
    // drops.
    let new_layout_relevant_count =
        layout_relevant_child_count(styled_dom, &new_children_dom_ids, new_dom_id);

    if std::env::var_os("AZ_RECON_DEBUG").is_some()
        && old_tree.is_some()
        && new_layout_relevant_count != old_layout_relevant_count
    {
        eprintln!(
            "[recon] COUNT MISMATCH parent dom {:?}: old_relevant={} new_relevant={} (direct_old={})",
            new_dom_id.index(),
            old_layout_relevant_count,
            new_layout_relevant_count,
            old_children_by_dom.len(),
        );
    }
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
            // css-flexbox-1 section 4 / css-grid-1 section 6: an anonymous
            // flex/grid item that contains only white space is not rendered.
            // Without this, every newline between a grid container's <div>
            // children became a real grid item and consumed an auto-placement
            // cell (grid-minmax-fr-001 rendered 0-height phantom items and
            // pushed real items into implicit rows).
            if parent_is_flex_or_grid
                && super::layout_tree::is_whitespace_only_text(styled_dom, new_child_dom_id)
            {
                continue;
            }
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
                subtree_style_changed,
                dom_diff_clean,
            )?;
            if let Some(child_node) = new_tree_builder.get(reconciled_child_idx) {
                new_child_hashes.push(child_node.subtree_hash.0);
            }

            if old_tree.and_then(|t| {
                t.cold(LayoutNodeId::new(old_child_idx?))
                    .map(|n| n.subtree_hash)
            }) != new_tree_builder
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
                                                               // Which inline run (== which anon-wrapper ordinal) we're on — the
                                                               // identity try_reuse_anon_wrapper matches against the old tree.
        let mut anon_ordinal: usize = 0;

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
                        let anon_reused = try_reuse_anon_wrapper(
                            old_tree,
                            old_tree_idx,
                            anon_ordinal,
                            &inline_run,
                            new_tree_builder,
                            anon_idx,
                        );
                        anon_ordinal += 1;

                        if let Some(msgs) = debug_messages.as_mut() {
                            msgs.push(LayoutDebugMessage::info(format!(
                            "[reconcile_recursive] Created anonymous IFC wrapper (layout_idx={}) for {} inline children: {:?}",
                            anon_idx,
                            inline_run.len(),
                            inline_run.iter().map(|(_, id)| id.index()).collect::<Vec<_>>()
                        )));
                        }

                        // Process each inline child under the anonymous wrapper
                        #[allow(clippy::iter_with_drain)]
                        // accumulator Vec reused across runs; drain(..) empties it while retaining the allocation
                        for (pos, inline_dom_id) in inline_run.drain(..) {
                            // Inline children live under the anon wrapper
                            // in the old tree, so the parent's direct
                            // `old_children_by_dom` map won't hit them.
                            // Fall through to the global `dom_to_layout`
                            // map; we don't care which anon wrapper they
                            // were under, only that their cold data
                            // (fingerprint) gets matched correctly.
                            let old_child_idx = old_children_by_dom
                                .get(&inline_dom_id)
                                .copied()
                                .or_else(|| {
                                    old_tree
                                        .and_then(|t| t.dom_to_layout.get(&inline_dom_id))
                                        .and_then(|v| v.first().copied().map(LayoutNodeId::index))
                                });
                            let reconciled_child_idx = reconcile_recursive(
                                styled_dom,
                                inline_dom_id,
                                old_child_idx,
                                Some(anon_idx), // Parent is the anonymous wrapper
                                old_tree,
                                new_tree_builder,
                                recon,
                                debug_messages,
                                subtree_style_changed,
                                dom_diff_clean,
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
                        // cached layout. `children_are_different` flips the
                        // parent to layout-dirty ONLY when the wrapper is
                        // genuinely new / its run changed — the previous
                        // unconditional `= true` here re-dirtied every
                        // paragraph's parent on every reconcile (see
                        // try_reuse_anon_wrapper).
                        if !anon_reused {
                            if std::env::var_os("AZ_RECON_DEBUG").is_some() {
                                eprintln!(
                                    "[recon] mid-loop wrapper ord {} NOT reused (run len {})",
                                    anon_ordinal - 1,
                                    inline_run.len()
                                );
                            }
                            children_are_different = true;
                        }
                    } // end else (non-whitespace run)
                }

                // Process block-level child directly under parent
                let old_child_idx = old_children_by_dom
                    .get(&new_child_dom_id)
                    .copied()
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
                    subtree_style_changed,
                    dom_diff_clean,
                )?;
                if let Some(child_node) = new_tree_builder.get(reconciled_child_idx) {
                    new_child_hashes.push(child_node.subtree_hash.0);
                }

                if old_tree.and_then(|t| {
                    t.cold(LayoutNodeId::new(old_child_idx?))
                        .map(|n| n.subtree_hash)
                }) != new_tree_builder
                    .get(reconciled_child_idx)
                    .map(|n| n.subtree_hash)
                {
                    if std::env::var_os("AZ_RECON_DEBUG").is_some() {
                        eprintln!(
                            "[recon] block child dom {:?} under parent dom {:?} hash MISMATCH warm_pass={} old_idx={:?} (old {:?} vs new {:?})",
                            new_child_dom_id.index(),
                            new_dom_id.index(),
                            old_tree.is_some(),
                            old_child_idx,
                            old_tree.and_then(|t| t.cold(LayoutNodeId::new(old_child_idx.unwrap_or(usize::MAX))).map(|n| n.subtree_hash)),
                            new_tree_builder.get(reconciled_child_idx).map(|n| n.subtree_hash),
                        );
                    }
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
                let anon_reused = try_reuse_anon_wrapper(
                    old_tree,
                    old_tree_idx,
                    anon_ordinal,
                    &inline_run,
                    new_tree_builder,
                    anon_idx,
                );
                anon_ordinal += 1;

                if let Some(msgs) = debug_messages.as_mut() {
                    msgs.push(LayoutDebugMessage::info(format!(
                    "[reconcile_recursive] Created trailing anonymous IFC wrapper (layout_idx={}) for {} inline children: {:?}",
                    anon_idx,
                    inline_run.len(),
                    inline_run.iter().map(|(_, id)| id.index()).collect::<Vec<_>>()
                )));
                }

                #[allow(clippy::iter_with_drain)]
                // accumulator Vec reused across runs; drain(..) empties it while retaining the allocation
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
                        subtree_style_changed,
                        dom_diff_clean,
                    )?;
                    if let Some(child_node) = new_tree_builder.get(reconciled_child_idx) {
                        new_child_hashes.push(child_node.subtree_hash.0);
                    }
                }

                // See note in main mixed-content branch: rely on
                // children's own mark_dirty to propagate upward rather
                // than invalidating the whole wrapper each reconcile.
                if !anon_reused {
                    if std::env::var_os("AZ_RECON_DEBUG").is_some() {
                        eprintln!(
                            "[recon] trailing wrapper ord {} NOT reused",
                            anon_ordinal - 1
                        );
                    }
                    children_are_different = true;
                }
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
        // Runtime-gated classification trace: names WHY a node went dirty,
        // which is the question every reconcile investigation starts with.
        if std::env::var_os("AZ_RECON_DEBUG").is_some() {
            eprintln!(
                "[recon] intrinsic_dirty += layout_idx {} (dom {:?}, flag {:?}, children_diff {})",
                new_node_idx,
                new_dom_id.index(),
                dirty_flag,
                children_are_different
            );
        }
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
    cb: &super::geometry::ContainingBlock,
) -> Result<PreparedLayoutContext<'a>> {
    // Legacy view for the consumers below that are still keyed on the
    // flattened form (cache slots, LayoutConstraints). Sizing itself gets the
    // TYPED cb so no arm of it can do arithmetic on a sentinel.
    let containing_block_size = cb.flattened();
    let node = tree
        .get(LayoutNodeId::new(node_index))
        .ok_or(LayoutError::InvalidTree)?;
    let warm = tree
        .warm(LayoutNodeId::new(node_index))
        .ok_or(LayoutError::InvalidTree)?;
    let dom_id = node.dom_node_id; // Can be None for anonymous boxes

    // Phase 1: Calculate this node's provisional used size

    // This size is based on the node's CSS properties (width, height, etc.) and
    // its containing block. If height is 'auto', this is a temporary value.
    let intrinsic = warm.intrinsic_sizes.unwrap_or_default();
    let final_used_size = calculate_used_size_for_node(
        ctx.styled_dom,
        dom_id, // Now Option<NodeId>
        cb,
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
        fragmentainer: None,
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
/// This is the single source of truth for scrollbar detection AT LAYOUT TIME.
/// Both the BFC path (`compute_scrollbar_info`) and the Taffy flex/grid path
/// (`compute_child_layout` in `taffy_bridge.rs`) call this function, ensuring
/// consistent behaviour. A `VirtualView`'s scrollable extent is not known yet
/// when this runs — [`apply_virtual_scroll_necessity`] amends the result after
/// layout and is the only other place a necessity flag is ever raised.
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
    let scrollbar_style =
        crate::solver3::getters::get_scrollbar_style_cached(ctx, dom_id, styled_node_state);
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
    // +spec:overflow:e8a828 - scrollbar-gutter affects gutter presence at the box's inline edges
    // +spec:overflow:3a6966 - classic scrollbar gutter width == scrollbar width; overlay scrollbars have no gutter
    //
    // NOT modeled: the non-normative side-selection note (overflow:3c44cc,
    // "which side a scrollbar appears on MAY depend on OS conventions or
    // bidirectionality") - the gutter is hardcoded to the physical right via
    // the width-only shrink. A "may", so the fixed choice is conformant; a
    // bidi-aware side needs writing-mode plumbing through ScrollbarReqs.
    //
    // scrollbar-gutter only applies to scroll containers (overflow: auto or scroll).
    // "stable" reserves gutter on the inline-end edge even if no scrollbar is needed.
    // "stable both-edges" reserves gutter on both inline edges.
    let scrollbar_gutter = get_scrollbar_gutter_property(ctx.styled_dom, dom_id, styled_node_state)
        .unwrap_or(azul_css::props::layout::overflow::StyleScrollbarGutter::Auto);
    let ob_y = to_overflow_behavior(overflow_y);
    let is_scroll_container = matches!(
        ob_y,
        fc::OverflowBehavior::Scroll | fc::OverflowBehavior::Auto
    );

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

/// Amends a node's [`ScrollbarRequirements`] with the `VirtualView` virtual
/// scroll size — the half of the necessity decision that only exists AFTER
/// layout.
///
/// `compute_scrollbar_info_core` decides from the LAID-OUT content size. A
/// `VirtualView` is a replaced element with no flow content, so its laid-out
/// content IS its viewport: `overflow: auto` can never fire for it, and the real
/// scrollable extent lives in `ScrollManager`'s `virtual_scroll_size`, which the
/// `VirtualView` callback publishes strictly after the layout pass that would
/// need to read it.
///
/// This is the ONE place that compares the two. Every consumer of
/// `needs_horizontal` / `needs_vertical` must reach the answer through it, or
/// the paint side and the GPU thumb updater disagree — a painted bar whose thumb
/// never moves. Two callers:
///
/// * `display_list::paint_scrollbars`, which cannot read a post-layout
///   write-back: the display list is built inside the very layout pass that
///   recomputes `warm.scrollbar_info` from scratch; and
/// * `shell2::common::layout::register_scroll_nodes`, which stores the amended
///   value back into `warm.scrollbar_info`, so that
///   `GpuStateManager::update_scrollbar_transforms`, the `ScrollManager`
///   registration and the hit-test scrollbar states all see the same answer.
///
/// `virtual_content_size` is `ScrollPosition::children_rect.size` — the same
/// number the thumb geometry is built from, so "is there a bar" and "how long is
/// its thumb" can never come from different sizes. `padding_box_size` is the
/// node's border box minus its borders: a `VirtualView` renders its child DOM
/// into its bounds, so that IS its viewport, and there is no flow content to
/// inset by padding.
///
/// Only the two booleans move. Layout has already run, so raising
/// `scrollbar_width` / `scrollbar_height` here would desync the paint from the
/// clip rect that reserved no gutter — an `auto` `VirtualView`'s bar overlays the
/// viewport's inner edge, while `overflow: scroll` keeps reserving its gutter at
/// layout time. Nothing is ever lowered: a bar layout already asked for stays.
///
/// Returns `true` if a flag was raised.
pub fn apply_virtual_scroll_necessity(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    virtual_content_size: LogicalSize,
    padding_box_size: LogicalSize,
    reqs: &mut ScrollbarRequirements,
) -> bool {
    let is_virtual_view = styled_dom
        .node_data
        .as_container()
        .get(dom_id)
        .is_some_and(azul_core::dom::NodeData::is_virtual_view_node);
    if !is_virtual_view {
        return false;
    }
    apply_content_scroll_necessity(
        styled_dom,
        dom_id,
        virtual_content_size,
        padding_box_size,
        reqs,
    )
}

/// The axis-raising core of [`apply_virtual_scroll_necessity`], without the
/// `VirtualView` gate: raise `needs_horizontal` / `needs_vertical` when the
/// node's resolved `overflow-x`/`overflow-y` allows user scrolling and
/// `content_size` exceeds the padding box.
///
/// `register_scroll_nodes` also calls this for ORDINARY nodes, because a text
/// edit can grow an IFC's content after layout: `reshape_text_node` re-runs
/// text3 and refreshes `overflow_content_size`, but nothing re-runs the
/// Phase-3 `compute_scrollbar_info` pass — so a single-line text input whose
/// value outgrew the field would keep `needs_horizontal == false` forever and
/// never register the scroll box the caret-reveal needs. For nodes whose
/// content did NOT change after layout this is a no-op (both sides read the
/// same `overflow_content_size` Phase 3 wrote). Nothing is ever lowered.
///
/// Returns `true` if a flag was raised.
pub fn apply_content_scroll_necessity(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    content_size: LogicalSize,
    padding_box_size: LogicalSize,
    reqs: &mut ScrollbarRequirements,
) -> bool {
    // Same tolerance as `check_scrollbar_necessity`: a sub-pixel difference must
    // not raise a bar.
    const EPSILON: f32 = 1.0;
    let virtual_content_size = content_size;

    let node_state = styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| n.styled_node_state)
        .unwrap_or_default();
    let raw_overflow_x = get_overflow_x(styled_dom, dom_id, &node_state);
    let raw_overflow_y = get_overflow_y(styled_dom, dom_id, &node_state);
    let overflow_x = raw_overflow_x.resolve_computed(&raw_overflow_y);
    let overflow_y = raw_overflow_y.resolve_computed(&raw_overflow_x);

    let raise_horizontal = !reqs.needs_horizontal
        && overflow_x.allows_user_scrolling()
        && virtual_content_size.width > padding_box_size.width + EPSILON;
    if raise_horizontal {
        reqs.needs_horizontal = true;
    }
    let raise_vertical = !reqs.needs_vertical
        && overflow_y.allows_user_scrolling()
        && virtual_content_size.height > padding_box_size.height + EPSILON;
    if raise_vertical {
        reqs.needs_vertical = true;
    }
    raise_horizontal || raise_vertical
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
///
/// A flip only counts when at least one of the two states RESERVES layout space —
/// the same criterion the no-previous-info arm has always used
/// (`ScrollbarRequirements::needs_reflow`). What a reflow buys is a corrected
/// available width/height (`layout_bfc`'s `scrollbar_reservation`, and
/// `shrink_size` below), so two zero-reservation states cannot need one. This
/// keeps overlay scrollbars (macOS-style, `reserve_width_px == 0`) from paying a
/// full extra layout pass every time a bar appears, and it is what stops
/// [`apply_virtual_scroll_necessity`]'s post-layout amendment — which raises
/// flags without reserving anything — from asking for one on every single pass
/// over an `overflow: auto` `VirtualView`.
fn check_scrollbar_change(
    tree: &LayoutTree,
    node_index: usize,
    scrollbar_info: &ScrollbarRequirements,
    skip_scrollbar_check: bool,
) -> bool {
    if skip_scrollbar_check {
        return false;
    }

    let Some(warm_node) = tree.warm(LayoutNodeId::new(node_index)) else {
        return false;
    };

    warm_node.scrollbar_info.as_ref().map_or_else(
        || scrollbar_info.needs_reflow(),
        |old_info| {
            // Trigger reflow if scrollbar state changed in either direction
            let horizontal_changed = old_info.needs_horizontal != scrollbar_info.needs_horizontal;
            let vertical_changed = old_info.needs_vertical != scrollbar_info.needs_vertical;
            (horizontal_changed || vertical_changed)
                && (old_info.needs_reflow() || scrollbar_info.needs_reflow())
        },
    )
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
        })
        .map_or_else(|| "Unknown".to_string(), |n| format!("{:?}", n.node_type));

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
        })
        .map_or_else(|| "Unknown".to_string(), |n| format!("{:?}", n.node_type));

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
    text_cache: &TextLayoutCache,
    child_index: usize,
    child_relative_pos: LogicalPosition,
    self_content_box_pos: LogicalPosition,
    inner_size_after_scrollbars: LogicalSize,
    writing_mode: LayoutWritingMode,
    is_flex_or_grid: bool,
    calculated_positions: &mut super::PositionVec,
    reflow_needed_for_scrollbars: bool,
    float_cache: &HashMap<usize, fc::FloatingContext>,
) -> Result<()> {
    // Set relative position on child
    // child_relative_pos is [CoordinateSpace::Parent] - relative to parent's content-box
    let child_warm = tree
        .warm_mut(LayoutNodeId::new(child_index))
        .ok_or(LayoutError::InvalidTree)?;
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
        let child_node = tree
            .get(LayoutNodeId::new(child_index))
            .ok_or(LayoutError::InvalidTree)?;
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
    let child_node = tree
        .get(LayoutNodeId::new(child_index))
        .ok_or(LayoutError::InvalidTree)?;
    let child_bp = child_node.box_props.unpack();
    let child_content_box_pos = calculate_content_box_pos(child_absolute_pos, &child_bp);
    let child_inner_size =
        child_bp.inner_size(child_node.used_size.unwrap_or_default(), writing_mode);
    let child_children: Vec<usize> = tree.children(child_index).to_vec();
    let child_fc = child_node.formatting_context;

    // Recurse to position grandchildren
    // OPTIMIZATION: For BFC/IFC children, layout_bfc/layout_ifc already computed their layout.
    // We just need to set absolute positions for descendants.
    // Only recurse if child has children to position.
    if !child_children.is_empty() {
        if is_flex_or_grid {
            // For Flex/Grid: Taffy already set used_size. Only recurse for grandchildren.
            position_flex_child_descendants(
                tree,
                child_index,
                child_content_box_pos,
                child_inner_size,
                calculated_positions,
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
pub(super) fn position_bfc_child_descendants(
    tree: &LayoutTree,
    node_index: usize,
    content_box_pos: LogicalPosition,
    calculated_positions: &mut super::PositionVec,
) {
    let Some(node) = tree.get(LayoutNodeId::new(node_index)) else {
        return;
    };

    for &child_index in tree.children(node_index) {
        let Some(child_node) = tree.get(LayoutNodeId::new(child_index)) else {
            continue;
        };

        // Use the relative_position that was set during formatting context layout
        let child_rel_pos = tree
            .warm(LayoutNodeId::new(child_index))
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
        position_bfc_child_descendants(
            tree,
            child_index,
            child_content_box_pos,
            calculated_positions,
        );
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
    cb: &super::geometry::ContainingBlock,
    calculated_positions: &mut super::PositionVec,
    reflow_needed_for_scrollbars: &mut bool,
    float_cache: &mut HashMap<usize, fc::FloatingContext>,
) -> Result<()> {
    // Collect out-of-flow children (those not already positioned)
    let out_of_flow_children: Vec<(usize, Option<NodeId>)> = {
        let current_node = tree
            .get(LayoutNodeId::new(node_index))
            .ok_or(LayoutError::InvalidTree)?;
        tree.children(node_index)
            .iter()
            .filter_map(|&child_index| {
                if super::pos_contains(calculated_positions, child_index) {
                    return None;
                }
                let child = tree.get(LayoutNodeId::new(child_index))?;
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
            cb,
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
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if laying out the subtree fails.
pub fn calculate_layout_for_subtree<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    containing_block_pos: LogicalPosition,
    cb: &super::geometry::ContainingBlock,
    calculated_positions: &mut super::PositionVec,
    reflow_needed_for_scrollbars: &mut bool,
    float_cache: &mut HashMap<usize, fc::FloatingContext>,
    compute_mode: ComputeMode,
) -> Result<()> {
    calculate_layout_for_subtree_fragment(
        ctx,
        tree,
        text_cache,
        node_index,
        containing_block_pos,
        cb,
        calculated_positions,
        reflow_needed_for_scrollbars,
        float_cache,
        compute_mode,
        None,
        None,
    )
}

/// [`calculate_layout_for_subtree`] + the K30b fragment channel: `fragment`
/// arms this subtree's fragmentainer (space + optional resume token, riding
/// `LayoutConstraints.fragmentainer` — design §4.3/4.4); `fragment_out`
/// receives the subtree's outgoing [`BreakToken`], `None` = finished.
/// Existing callers use the plain wrapper (both channels `None` — the
/// continuous path is bit-identical by construction).
#[allow(clippy::too_many_arguments)]
pub fn calculate_layout_for_subtree_fragment<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    containing_block_pos: LogicalPosition,
    cb: &super::geometry::ContainingBlock,
    calculated_positions: &mut super::PositionVec,
    reflow_needed_for_scrollbars: &mut bool,
    float_cache: &mut HashMap<usize, fc::FloatingContext>,
    compute_mode: ComputeMode,
    fragment: Option<fc::FragmentainerSpace<'_>>,
    mut fragment_out: Option<&mut Option<crate::solver3::break_token::BreakToken>>,
) -> Result<()> {
    // Legacy flattened view for the cache keys and the not-yet-migrated
    // consumers in this body; the typed `cb` travels to sizing and recursion.
    let containing_block_size = cb.flattened();
    // [g147b az-web-lift DIAG] per-node calculate_layout_for_subtree entry (0x60980+slot): records the
    // last compute_mode that reached this node (PerformLayout=2 wins, runs after ComputeSize=1). If a div
    // shows 0x...0002 here but its layout_formatting_context marker (0x609A0+) is UNSET → positioning
    // reached calculate but short-circuited (cache hit) before dispatching to the formatting context.
    #[cfg(feature = "web_lift")]
    unsafe {
        let m = match compute_mode {
            ComputeMode::PerformLayout => 0xC0DE0002u32,
            _ => 0xC0DE0001u32,
        };
        crate::az_mark((0x60980 + (node_index & 7) * 4) as u32, (m) as u32);
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
    // NG rule (design doc §1.3 / research §2.1): the layout-result cache is
    // BYPASSED for fragment passes — the fragmentainer (remaining extent +
    // resume token) is not part of the cache key, so a hit would return the
    // CONTINUOUS geometry and silently skip breaking. Fragment results are
    // also never STORED (they would poison the continuous cache).
    if fragment.is_none() && node_index < ctx.cache_map.entries.len() {
        match compute_mode {
            ComputeMode::ComputeSize => {
                // ComputeSize: check the measurement slot for THIS constraint
                // shape (Taffy's 9-slot scheme, wired 2026-08-08). The measure
                // visit (indefinite height) and the final visit (definite
                // height) land in different slots, so neither evicts the
                // other — the slot-0 collapse made every visit a miss.
                let (size_slot, size_key) = NodeCache::classify_size_key(containing_block_size);
                let sizing_hit = ctx.cache_map.entries[node_index]
                    .get_size(size_slot, size_key)
                    .copied();
                if let Some(cached_sizing) = sizing_hit {
                    // SIZING CACHE HIT — set used_size and return immediately.
                    // No child positioning needed in ComputeSize mode.
                    drop(crate::probe::Probe::span("size_cache_hit_sizing"));
                    if let Some(node) = tree.get_mut(LayoutNodeId::new(node_index)) {
                        node.used_size = Some(cached_sizing.result_size);
                    }
                    if let Some(warm) = tree.warm_mut(LayoutNodeId::new(node_index)) {
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
                    if let Some(node) = tree.get_mut(LayoutNodeId::new(node_index)) {
                        node.used_size = Some(cached_layout.result_size);
                    }
                    if let Some(warm) = tree.warm_mut(LayoutNodeId::new(node_index)) {
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
                unsafe {
                    crate::az_mark((0x60A60 + (node_index & 7) * 4) as u32, (0xC0DE0001) as u32);
                }
                // Miss triage (AZ_PROFILE=cpu): a steady-state resize showed
                // 1020/1020 misses — this names WHICH failure it is. "empty"
                // = slot never stored / cleared (invalidation or remap loss);
                // "sizekey" = an entry exists but was stored under different
                // constraints (the slot-0 collapse overwriting, or a genuine
                // containing-block change); "laykey" = only the layout slot
                // exists and its key mismatches.
                {
                    let e = &ctx.cache_map.entries[node_index];
                    let reason = if let Some(m) = e.measure_entries[size_slot].as_ref() {
                        let dw = (size_key.width - m.available_size.width).abs();
                        let dh = (size_key.height - m.available_size.height).abs();
                        if dw >= CACHE_SIZE_EPSILON && dh >= CACHE_SIZE_EPSILON {
                            "size_cache_miss_sizekey_both"
                        } else if dw >= CACHE_SIZE_EPSILON {
                            "size_cache_miss_sizekey_w"
                        } else {
                            // Which SIDE of the height pair is the indefinite
                            // sentinel? q∞/s∞ = the measure-vs-final ping-pong
                            // (one slot alternating between an indefinite
                            // measure query and a definite final query);
                            // finite/finite = a genuinely changed containing
                            // block height.
                            const BIG: f32 = 1.0e9;
                            let qi = size_key.height >= BIG || !size_key.height.is_finite();
                            let si = m.available_size.height >= BIG
                                || !m.available_size.height.is_finite();
                            match (qi, si) {
                                (true, false) => "size_cache_miss_sizekey_h_qinf",
                                (false, true) => "size_cache_miss_sizekey_h_sinf",
                                (true, true) => "size_cache_miss_sizekey_h_bothinf",
                                (false, false) => "size_cache_miss_sizekey_h_finite",
                            }
                        }
                    } else if e.layout_entry.is_some() {
                        "size_cache_miss_laykey"
                    } else {
                        "size_cache_miss_empty"
                    };
                    drop(crate::probe::Probe::span(reason));
                }
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
                    if let Some(node) = tree.get_mut(LayoutNodeId::new(node_index)) {
                        node.used_size = Some(cached_layout.result_size);
                    }
                    if let Some(warm) = tree.warm_mut(LayoutNodeId::new(node_index)) {
                        warm.overflow_content_size = Some(cached_layout.content_size);
                        warm.scrollbar_info = Some(cached_layout.scrollbar_info);
                    }

                    let box_props = tree
                        .get(LayoutNodeId::new(node_index))
                        .map(|n| n.box_props.unpack())
                        .unwrap_or_default();
                    let writing_mode = tree
                        .warm(LayoutNodeId::new(node_index))
                        .map(|w| w.computed_style.writing_mode)
                        .unwrap_or_default();
                    let self_content_box_pos =
                        calculate_content_box_pos(containing_block_pos, &box_props);

                    // Apply cached child positions and recurse
                    let result_size = cached_layout.result_size;
                    for (child_index, child_relative_pos) in &cached_layout.child_positions {
                        let child_abs_pos = LogicalPosition::new(
                            self_content_box_pos.x + child_relative_pos.x,
                            self_content_box_pos.y + child_relative_pos.y,
                        );
                        super::pos_set(calculated_positions, *child_index, child_abs_pos);

                        let inner = box_props.inner_size(result_size, writing_mode);
                        // Subtract scrollbar reservation from the available size
                        // passed to children. This mirrors what layout_bfc does in
                        // the MISS path — without it, a reflow-loop cache hit
                        // would hand children the full content-box width, ignoring
                        // any vertical/horizontal scrollbar that was detected.
                        let child_available_size = cached_layout.scrollbar_info.shrink_size(inner);
                        // A cache-hit parent has a RESOLVED content box, so the
                        // children's containing block is fully definite.
                        let child_cb =
                            super::geometry::ContainingBlock::definite(child_available_size);
                        calculate_layout_for_subtree(
                            ctx,
                            tree,
                            text_cache,
                            *child_index,
                            child_abs_pos,
                            &child_cb,
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
        mut constraints,
        dom_id,
        writing_mode,
        mut final_used_size,
        box_props,
    } = {
        let _p = crate::probe::Probe::span("prepare_layout_context");
        prepare_layout_context(ctx, tree, node_index, cb)?
    };
    // K30b: arm this subtree's fragmentainer (None on the continuous path).
    if let Some(fs) = fragment {
        constraints.fragmentainer = Some(fs);
    }

    // Phase 1.5: Update used_size BEFORE calling layout_formatting_context.
    //
    // When a node is cloned from the old tree (clone_node_from_old), its used_size
    // retains the value from the previous layout pass. If the containing block changed
    // (e.g. viewport resize), the stale used_size would cause layout_bfc() to compute
    // an incorrect children_containing_block_size. By updating used_size here, we ensure
    // that layout_bfc reads the freshly resolved size from prepare_layout_context.
    {
        let is_table_cell = tree
            .get(LayoutNodeId::new(node_index))
            .is_some_and(|n| matches!(n.formatting_context, FormattingContext::TableCell));
        if !is_table_cell {
            if let Some(node) = tree.get_mut(LayoutNodeId::new(node_index)) {
                node.used_size = Some(final_used_size);
            }
        }
    }

    // Phase 2: Layout children using the formatting context
    let layout_result = {
        let _p = crate::probe::Probe::span("layout_formatting_context");
        layout_formatting_context(ctx, tree, text_cache, node_index, &constraints, float_cache)?
    };
    // K30b: hand the subtree's resume state up (None = finished).
    if let Some(slot) = fragment_out {
        slot.clone_from(&layout_result.outgoing_token);
    }
    let content_size = layout_result.output.overflow_size;

    // If layout_formatting_context adjusted this node's used_size (e.g.
    // layout_flex_grid auto-applying box-sizing:border-box on the root),
    // propagate that back into final_used_size so Phase 3 (scrollbars),
    // Phase 4 (final write), and the self_content_box_pos calculation all
    // see the same border-box that the children were laid out inside.
    if let Some(adjusted) = tree
        .get(LayoutNodeId::new(node_index))
        .and_then(|n| n.used_size)
    {
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
    // A box whose BLOCK (height) axis scrolls must NOT expand to fit content —
    // its height comes from the containing block and the overflow scrolls. But
    // that gate is per-AXIS: `overflow-x` / `overflow-y` are PHYSICAL, so only a
    // VERTICAL scroll container (`overflow-y: scroll|auto`) fixes the height.
    // A purely HORIZONTAL scroll container — e.g. a single-line text field with
    // `overflow-x: auto, overflow-y: hidden` — must still grow its height to the
    // text line. Gating on EITHER axis collapsed such a field to zero height, so
    // its overflowing line was never measured and it never became a scroll box
    // the caret-reveal could shift (the append-only caret bug).
    //
    // Exception: if the containing block height is infinite (unconstrained),
    // we must still grow, since you can't scroll inside an infinitely tall box.
    let scrolls_vertically = dom_id.is_some_and(|id| {
        let ov_y = get_overflow_y(ctx.styled_dom, id, &styled_node_state);
        matches!(
            ov_y,
            MultiValue::Exact(LayoutOverflow::Scroll | LayoutOverflow::Auto)
        )
    });

    if should_use_content_height(&css_height) {
        let skip_expansion = scrolls_vertically
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
    let scrollbar_info = dom_id.map_or_else(ScrollbarRequirements::default, |id| {
        compute_scrollbar_info(
            ctx,
            id,
            &styled_node_state,
            content_size,
            &box_props,
            final_used_size,
            writing_mode,
        )
    });

    if check_scrollbar_change(tree, node_index, &scrollbar_info, skip_scrollbar_check) {
        *reflow_needed_for_scrollbars = true;
    }

    let merged_scrollbar_info = scrollbar_info;
    let content_box_size = box_props.inner_size(final_used_size, writing_mode);
    let inner_size_after_scrollbars = merged_scrollbar_info.shrink_size(content_box_size);

    // Phase 4: Update this node's state
    let self_content_box_pos = {
        {
            let current_node = tree
                .get_mut(LayoutNodeId::new(node_index))
                .ok_or(LayoutError::InvalidTree)?;

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
        if let Some(warm) = tree.warm_mut(LayoutNodeId::new(node_index)) {
            warm.scrollbar_info = Some(merged_scrollbar_info);
            // Store overflow content size for scroll frame calculation
            // +spec:overflow:f28d6a - hanging glyphs should be ink overflow, not scrollable overflow (not yet subtracted from content_size)
            warm.overflow_content_size = Some(content_size);
        }

        // self_content_box_pos is [CoordinateSpace::Window] - the absolute position of this node's content-box
        let current_node = tree
            .get(LayoutNodeId::new(node_index))
            .ok_or(LayoutError::InvalidTree)?;
        let current_bp = current_node.box_props.unpack();
        let pos = calculate_content_box_pos(containing_block_pos, &current_bp);
        log_content_box_calculation(ctx, node_index, current_node, containing_block_pos, pos);
        pos
    };

    // Phase 5: Determine formatting context type
    let is_flex_or_grid = {
        let node = tree
            .get(LayoutNodeId::new(node_index))
            .ok_or(LayoutError::InvalidTree)?;
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
            *reflow_needed_for_scrollbars,
            float_cache,
        )?;
    }

    // Phase 7: Process out-of-flow children (absolute/fixed). The node's own
    // used size is resolved by now, so the abs-pos containing block is
    // definite regardless of the constraint this node was measured under.
    process_out_of_flow_children(
        ctx,
        tree,
        text_cache,
        node_index,
        self_content_box_pos,
        &super::geometry::ContainingBlock::definite(inner_size_after_scrollbars),
        calculated_positions,
        reflow_needed_for_scrollbars,
        float_cache,
    )?;

    // === STORE RESULT IN PER-NODE CACHE (Taffy-inspired 9+1 slot cache) ===
    // Store both the full layout entry and a sizing measurement entry.
    // This enables O(n) two-pass BFC: Pass 1 populates cache, Pass 2 reads it.
    // Fragment passes never store (NG rule, same as the hit-side gate above:
    // fragment geometry would poison the continuous cache).
    if fragment.is_none() && node_index < ctx.cache_map.entries.len() {
        let warm_ref = tree.warm(LayoutNodeId::new(node_index));
        let baseline = warm_ref.and_then(|n| n.baseline);
        let escaped_top = warm_ref.and_then(|n| n.escaped_top_margin);
        let escaped_bottom = warm_ref.and_then(|n| n.escaped_bottom_margin);

        // Store in the layout slot (PerformLayout result)
        ctx.cache_map
            .get_mut(node_index)
            .store_layout(LayoutCacheEntry {
                available_size: containing_block_size,
                result_size: final_used_size,
                content_size,
                child_positions: child_positions_for_cache,
                escaped_top_margin: escaped_top,
                escaped_bottom_margin: escaped_bottom,
                scrollbar_info: merged_scrollbar_info,
            });

        // Also store in the measurement slot matching THIS constraint shape
        // (same classification as the lookup — the measure visit and the
        // final visit must land in different slots or they evict each other
        // every pass). The canonicalized key replaces the raw containing
        // block so an indefinite axis compares equal next time.
        let (size_slot, size_key) = NodeCache::classify_size_key(containing_block_size);
        ctx.cache_map.get_mut(node_index).store_size(
            size_slot,
            SizingCacheEntry {
                available_size: size_key,
                result_size: final_used_size,
                baseline,
                escaped_top_margin: escaped_top,
                escaped_bottom_margin: escaped_bottom,
            },
        );
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
fn position_flex_child_descendants(
    tree: &mut LayoutTree,
    node_index: usize,
    content_box_pos: LogicalPosition,
    available_size: LogicalSize,
    calculated_positions: &mut super::PositionVec,
) -> Result<()> {
    let children: Vec<usize> = tree.children(node_index).to_vec();

    for &child_index in &children {
        let child_node = tree
            .get(LayoutNodeId::new(child_index))
            .ok_or(LayoutError::InvalidTree)?;
        let child_rel_pos = tree
            .warm(LayoutNodeId::new(child_index))
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
            .warm(LayoutNodeId::new(child_index))
            .map(|w| w.computed_style.writing_mode)
            .unwrap_or_default();
        let child_content_box = LogicalPosition::new(
            child_abs_pos.x + cbp.border.left + cbp.padding.left,
            child_abs_pos.y + cbp.border.top + cbp.padding.top,
        );
        let child_inner_size =
            cbp.inner_size(child_node.used_size.unwrap_or_default(), child_writing_mode);

        // Recurse
        position_flex_child_descendants(
            tree,
            child_index,
            child_content_box,
            child_inner_size,
            calculated_positions,
        )?;
    }

    Ok(())
}

/// Checks if the given CSS height value should use content-based sizing
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
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
    let node_props = tree
        .get(LayoutNodeId::new(node_index))
        .ok_or(LayoutError::InvalidTree)?
        .box_props
        .unpack();
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

#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn compute_counters_recursive(
    styled_dom: &StyledDom,
    tree: &LayoutTree,
    node_idx: usize,
    counters: &mut HashMap<(usize, String), i32>,
    counter_stacks: &mut HashMap<String, Vec<i32>>,
    scope_stack: &mut Vec<Vec<String>>,
) {
    let Some(node) = tree.get(LayoutNodeId::new(node_idx)) else {
        return;
    };

    // Skip pseudo-elements (::marker, ::before, ::after) for counter processing
    // Pseudo-elements inherit counter values from their parent element
    // but don't participate in counter-reset or counter-increment themselves
    if tree
        .warm(LayoutNodeId::new(node_idx))
        .and_then(|w| w.pseudo_element.as_ref())
        .is_some()
    {
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
        && cache
            .compact_cache
            .as_ref()
            .is_none_or(|cc| cc.has_counter(dom_id.index()));

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

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod autotest_generated {
    use azul_core::dom::{Dom, IdOrClass};
    use azul_css::props::{
        basic::{pixel::PixelValue, SizeMetric},
        layout::dimensions::CalcAstItemVec,
    };

    use super::*;
    use crate::solver3::{
        display_list::DisplayList,
        geometry::{EdgeSizes, MarginAuto, PackedBoxProps, ResolvedBoxProps},
        layout_tree::{LayoutNodeCold, LayoutNodeWarm},
        pos_get, PositionVec, POSITION_UNSET,
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    fn size(w: f32, h: f32) -> LogicalSize {
        LogicalSize::new(w, h)
    }

    fn pos(x: f32, y: f32) -> LogicalPosition {
        LogicalPosition::new(x, y)
    }

    fn sizing_entry(available: LogicalSize, result: LogicalSize) -> SizingCacheEntry {
        SizingCacheEntry {
            available_size: available,
            result_size: result,
            baseline: None,
            escaped_top_margin: None,
            escaped_bottom_margin: None,
        }
    }

    fn layout_entry(available: LogicalSize, result: LogicalSize) -> LayoutCacheEntry {
        LayoutCacheEntry {
            available_size: available,
            result_size: result,
            content_size: result,
            child_positions: Vec::new(),
            escaped_top_margin: None,
            escaped_bottom_margin: None,
            scrollbar_info: ScrollbarRequirements::default(),
        }
    }

    fn edges(top: f32, right: f32, bottom: f32, left: f32) -> EdgeSizes {
        EdgeSizes {
            top,
            right,
            bottom,
            left,
        }
    }

    fn box_props(margin: EdgeSizes, border: EdgeSizes, padding: EdgeSizes) -> ResolvedBoxProps {
        ResolvedBoxProps {
            margin,
            padding,
            border,
            margin_auto: MarginAuto::default(),
        }
    }

    fn zero_box_props() -> ResolvedBoxProps {
        box_props(
            edges(0.0, 0.0, 0.0, 0.0),
            edges(0.0, 0.0, 0.0, 0.0),
            edges(0.0, 0.0, 0.0, 0.0),
        )
    }

    fn hot(
        parent: Option<usize>,
        dom_node_id: Option<NodeId>,
        used_size: Option<LogicalSize>,
        bp: &ResolvedBoxProps,
    ) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: PackedBoxProps::pack(bp),
            dom_node_id,
            used_size,
            formatting_context: FormattingContext::Block {
                establishes_new_context: false,
            },
            parent,
        }
    }

    /// Plain hot node with no box props and no DOM id.
    fn plain(parent: Option<usize>) -> LayoutNodeHot {
        hot(parent, None, Some(size(0.0, 0.0)), &zero_box_props())
    }

    /// Builds a `LayoutTree` from hot nodes + per-node child lists.
    /// `child_lists[i]` are the children of node `i`.
    fn build_tree(
        nodes: Vec<LayoutNodeHot>,
        warm: Vec<LayoutNodeWarm>,
        child_lists: &[Vec<usize>],
    ) -> LayoutTree {
        let n = nodes.len();
        let mut children_arena: Vec<usize> = Vec::new();
        let mut children_offsets: Vec<(u32, u32)> = Vec::with_capacity(n);
        for cl in child_lists {
            let start = u32::try_from(children_arena.len()).unwrap();
            children_arena.extend_from_slice(cl);
            children_offsets.push((start, u32::try_from(cl.len()).unwrap()));
        }
        while children_offsets.len() < n {
            children_offsets.push((0, 0));
        }
        LayoutTree {
            nodes,
            warm,
            cold: vec![LayoutNodeCold::default(); n],
            root: 0,
            dom_to_layout: BTreeMap::new(),
            children_arena,
            children_offsets,
            subtree_needs_intrinsic: Vec::new(),
        }
    }

    fn warm_default(n: usize) -> Vec<LayoutNodeWarm> {
        vec![LayoutNodeWarm::default(); n]
    }

    fn div_class(class: &str) -> Dom {
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
    }

    fn styled(dom: Dom, css_str: &str) -> StyledDom {
        let mut dom = dom;
        let (css, _warnings) = azul_css::parser2::new_from_str(css_str);
        StyledDom::create(&mut dom, css)
    }

    /// `body(0) > .p(1) > [ text " \n\t"(2), text "hi"(3), div(4), text NBSP(5) ]`
    ///
    /// DOM ids follow the depth-first pre-order numbering of `CompactDom`.
    fn whitespace_dom(css_str: &str) -> StyledDom {
        styled(
            Dom::create_body().with_child(
                div_class("p")
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        " \n\t",
                    ))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "hi",
                    ))
                    .with_child(Dom::create_div())
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "\u{00A0}",
                    )),
            ),
            css_str,
        )
    }

    // ==================================================================
    // NodeCache — slot cache (numeric / round-trip)
    // ==================================================================

    #[test]
    fn nodecache_default_is_empty_and_never_hits() {
        let c = NodeCache::default();
        assert!(c.is_empty);
        assert!(c.layout_entry.is_none());
        assert!(c.measure_entries.iter().all(Option::is_none));
        for slot in 0..9 {
            assert!(c.get_size(slot, size(0.0, 0.0)).is_none());
            assert!(c.get_size(slot, size(100.0, 100.0)).is_none());
        }
        assert!(c.get_layout(size(0.0, 0.0)).is_none());
    }

    #[test]
    fn nodecache_store_size_then_exact_lookup_round_trips() {
        let mut c = NodeCache::default();
        c.store_size(0, sizing_entry(size(200.0, 100.0), size(50.0, 40.0)));
        assert!(!c.is_empty);

        let hit = c.get_size(0, size(200.0, 100.0)).expect("exact hit");
        assert_eq!(hit.available_size, size(200.0, 100.0));
        assert_eq!(hit.result_size, size(50.0, 40.0));
    }

    #[test]
    fn nodecache_get_size_result_matches_request() {
        // Taffy's key optimization: Pass 2 hands back the size Pass 1 produced.
        let mut c = NodeCache::default();
        c.store_size(0, sizing_entry(size(200.0, 100.0), size(50.0, 40.0)));

        let hit = c
            .get_size(0, size(50.0, 40.0))
            .expect("result-matches-request hit");
        assert_eq!(hit.result_size, size(50.0, 40.0));
        // A size matching neither the request nor the result must miss.
        assert!(c.get_size(0, size(51.0, 41.0)).is_none());
    }

    #[test]
    fn nodecache_get_size_epsilon_boundary() {
        let mut c = NodeCache::default();
        c.store_size(0, sizing_entry(size(100.0, 100.0), size(10.0, 10.0)));

        // Sub-epsilon drift on either axis is still a hit (CACHE_SIZE_EPSILON = 0.1).
        assert!(c.get_size(0, size(100.05, 99.95)).is_some());
        // A drift clearly past the epsilon is a miss on both the request and the
        // result comparison. (The exact `== EPSILON` boundary is deliberately not
        // asserted: `100.1f32 - 100.0f32` rounds to 0.09999847, just under it.)
        assert!(c.get_size(0, size(100.2, 100.0)).is_none());
        assert!(c.get_size(0, size(100.0, 99.5)).is_none());
    }

    #[test]
    fn nodecache_get_size_nan_request_misses_instead_of_panicking() {
        let mut c = NodeCache::default();
        c.store_size(0, sizing_entry(size(100.0, 100.0), size(10.0, 10.0)));

        // NaN - x = NaN, and every NaN comparison is false → miss, not a hit.
        assert!(c.get_size(0, size(f32::NAN, 100.0)).is_none());
        assert!(c.get_size(0, size(100.0, f32::NAN)).is_none());
        assert!(c.get_size(0, size(f32::NAN, f32::NAN)).is_none());
    }

    #[test]
    fn nodecache_nan_and_infinite_entries_are_unreachable() {
        // An entry stored with a non-finite available/result size can never be
        // hit again (inf - inf = NaN, NaN - NaN = NaN) — the node simply gets
        // re-measured. That is safe, but it means such slots are dead weight.
        let mut c = NodeCache::default();
        c.store_size(
            0,
            sizing_entry(
                size(f32::INFINITY, f32::INFINITY),
                size(f32::INFINITY, f32::INFINITY),
            ),
        );
        assert!(c.get_size(0, size(f32::INFINITY, f32::INFINITY)).is_none());

        c.store_size(
            1,
            sizing_entry(size(f32::NAN, f32::NAN), size(f32::NAN, f32::NAN)),
        );
        assert!(c.get_size(1, size(f32::NAN, f32::NAN)).is_none());
        // ...but the cache still reports itself as populated.
        assert!(!c.is_empty);
    }

    #[test]
    fn nodecache_handles_zero_and_negative_sizes() {
        let mut c = NodeCache::default();
        c.store_size(0, sizing_entry(size(0.0, 0.0), size(0.0, 0.0)));
        assert!(c.get_size(0, size(0.0, 0.0)).is_some());
        assert!(c.get_size(0, size(-0.0, -0.0)).is_some());

        c.store_size(1, sizing_entry(size(-100.0, -50.0), size(-1.0, -1.0)));
        let hit = c
            .get_size(1, size(-100.0, -50.0))
            .expect("negative sizes are deterministic");
        assert_eq!(hit.result_size, size(-1.0, -1.0));
    }

    #[test]
    fn nodecache_extreme_finite_sizes_do_not_panic() {
        let mut c = NodeCache::default();
        c.store_size(
            0,
            sizing_entry(size(f32::MAX, f32::MIN), size(f32::MAX, f32::MIN)),
        );
        // MAX - MAX == 0 → exact hit; no overflow panic on the subtraction.
        assert!(c.get_size(0, size(f32::MAX, f32::MIN)).is_some());
        // MIN - MAX overflows to -inf, abs() = inf, inf < 0.1 is false → miss.
        assert!(c.get_size(0, size(f32::MIN, f32::MAX)).is_none());
    }

    #[test]
    fn nodecache_slots_are_independent() {
        let mut c = NodeCache::default();
        c.store_size(0, sizing_entry(size(1.0, 1.0), size(9.0, 9.0)));
        c.store_size(8, sizing_entry(size(2.0, 2.0), size(7.0, 7.0)));

        assert!(c.get_size(0, size(1.0, 1.0)).is_some());
        assert!(c.get_size(8, size(2.0, 2.0)).is_some());
        // No cross-talk between slots on the AVAILABLE-SIZE key: a query
        // whose constraints were stored in another slot does not hit here
        // (result sizes chosen so the result-matches fallback stays silent).
        assert!(c.get_size(0, size(2.0, 2.0)).is_none());
        assert!(c.get_size(8, size(1.0, 1.0)).is_none());
        assert!(c.get_size(4, size(1.0, 1.0)).is_none());
    }

    #[test]
    fn nodecache_result_match_crosses_slots_for_definite_queries() {
        // The pass1→pass2 handoff (2026-08-08 slot wiring): pass 1 measures
        // under an indefinite constraint and stores in a MEASURE slot; pass 2
        // queries with the measured result as a DEFINITE constraint — which
        // classifies to a different slot. "Result matches request" must
        // therefore scan ALL slots (Taffy's Cache::get does the same), or
        // wiring the 9 slots would have traded the eviction bug for a
        // permanent pass-2 miss.
        let mut c = NodeCache::default();
        // Stored under slot 1 (definite width, indefinite height).
        c.store_size(1, sizing_entry(size(100.0, f32::MAX), size(100.0, 40.0)));
        // Pass 2 asks with the RESULT as a fully-definite constraint → slot 0
        // is empty, but the slot-1 entry's result matches the request.
        let hit = c
            .get_size(0, size(100.0, 40.0))
            .expect("cross-slot result match");
        assert_eq!(hit.result_size, size(100.0, 40.0));
        // An INDEFINITE query never takes the fallback: only exact key hits.
        assert!(c.get_size(0, size(100.0, f32::MAX)).is_none());
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn nodecache_get_size_slot_out_of_range_panics() {
        // There are exactly 9 measurement slots; slot 9 is a caller bug and is
        // reported as an index panic rather than silently returning None.
        let c = NodeCache::default();
        let _ = c.get_size(9, size(0.0, 0.0));
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn nodecache_store_size_slot_out_of_range_panics() {
        let mut c = NodeCache::default();
        c.store_size(9, sizing_entry(size(1.0, 1.0), size(1.0, 1.0)));
    }

    #[test]
    fn nodecache_layout_slot_round_trips_and_matches_result() {
        let mut c = NodeCache::default();
        c.store_layout(layout_entry(size(800.0, 600.0), size(800.0, 123.0)));
        assert!(!c.is_empty);

        assert!(c.get_layout(size(800.0, 600.0)).is_some());
        // "Result matches request" applies to the layout slot too.
        let hit = c
            .get_layout(size(800.0, 123.0))
            .expect("result-matches-request");
        assert_eq!(hit.result_size, size(800.0, 123.0));
        assert!(c.get_layout(size(400.0, 300.0)).is_none());
        assert!(c.get_layout(size(f32::NAN, f32::NAN)).is_none());
    }

    #[test]
    fn nodecache_clear_wipes_every_slot() {
        let mut c = NodeCache::default();
        for slot in 0..9 {
            c.store_size(slot, sizing_entry(size(10.0, 10.0), size(10.0, 10.0)));
        }
        c.store_layout(layout_entry(size(10.0, 10.0), size(10.0, 10.0)));
        assert!(!c.is_empty);

        c.clear();

        assert!(c.is_empty);
        assert!(c.layout_entry.is_none());
        assert!(c.measure_entries.iter().all(Option::is_none));
        assert!(c.get_size(0, size(10.0, 10.0)).is_none());
        assert!(c.get_layout(size(10.0, 10.0)).is_none());

        // Clearing twice is harmless.
        c.clear();
        assert!(c.is_empty);
    }

    #[test]
    fn slot_index_always_lands_in_range_and_partitions_the_unknown_case() {
        use AvailableWidthType::{Definite, MaxContent, MinContent};
        let types = [Definite, MinContent, MaxContent];

        for &wt in &types {
            for &ht in &types {
                for wk in [true, false] {
                    for hk in [true, false] {
                        let slot = NodeCache::slot_index(wk, hk, wt, ht);
                        assert!(slot < 9, "slot {slot} out of the 9-slot range");
                    }
                }
            }
        }

        // Both known → always slot 0, regardless of the constraint types.
        for &wt in &types {
            for &ht in &types {
                assert_eq!(NodeCache::slot_index(true, true, wt, ht), 0);
            }
        }

        // Neither known → the 4 MinContent combos partition slots 5..=8.
        let mut neither: Vec<usize> = Vec::new();
        for &wt in &[Definite, MinContent] {
            for &ht in &[Definite, MinContent] {
                neither.push(NodeCache::slot_index(false, false, wt, ht));
            }
        }
        neither.sort_unstable();
        assert_eq!(neither, vec![5, 6, 7, 8]);
    }

    #[test]
    fn slot_index_collapses_definite_and_maxcontent_onto_one_slot() {
        use AvailableWidthType::{Definite, MaxContent, MinContent};
        // Documented: "MaxContent/Definite vs MinContent" share a slot.
        assert_eq!(
            NodeCache::slot_index(true, false, Definite, Definite),
            NodeCache::slot_index(true, false, MaxContent, Definite)
        );
        assert_ne!(
            NodeCache::slot_index(true, false, Definite, Definite),
            NodeCache::slot_index(true, false, MinContent, Definite)
        );
    }

    #[test]
    fn slot_index_keys_the_single_unknown_axis_off_the_known_axis_type() {
        use AvailableWidthType::{Definite, MinContent};
        // BEHAVIOUR PIN (deviates from Taffy): when only the width is known, the
        // slot is chosen from `width_type` — the type of the *known* axis — even
        // though the doc comment says it keys off "the unknown dimension(s)".
        // Taffy keys slot 1/2 off the height (the unknown axis) here. Same for
        // the mirrored (false, true) case, which keys off `height_type`.
        // Consequence: the height's MinContent-ness cannot select slot 2 at all,
        // so a MinContent and a Definite height measurement would collide in a
        // single slot once slots 1-8 are wired up (they are unused today).
        assert_eq!(NodeCache::slot_index(true, false, Definite, MinContent), 1);
        assert_eq!(NodeCache::slot_index(true, false, MinContent, Definite), 2);
        assert_eq!(NodeCache::slot_index(false, true, MinContent, Definite), 3);
        assert_eq!(NodeCache::slot_index(false, true, Definite, MinContent), 4);
    }

    // ==================================================================
    // LayoutCacheMap
    // ==================================================================

    #[test]
    fn cachemap_resize_to_tree_grows_shrinks_and_zeroes() {
        let mut m = LayoutCacheMap::default();
        assert!(m.entries.is_empty());

        m.resize_to_tree(0);
        assert!(m.entries.is_empty());

        m.resize_to_tree(3);
        assert_eq!(m.entries.len(), 3);
        assert!(m.entries.iter().all(|e| e.is_empty));

        // Populated entries survive a grow; new entries are dirty.
        m.get_mut(1)
            .store_size(0, sizing_entry(size(5.0, 5.0), size(5.0, 5.0)));
        m.resize_to_tree(5);
        assert_eq!(m.entries.len(), 5);
        assert!(!m.get(1).is_empty);
        assert!(m.get(4).is_empty);

        // Shrink drops the tail.
        m.resize_to_tree(1);
        assert_eq!(m.entries.len(), 1);
        m.resize_to_tree(0);
        assert!(m.entries.is_empty());
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn cachemap_get_out_of_range_panics() {
        let m = LayoutCacheMap::default();
        let _ = m.get(0);
    }

    #[test]
    fn cachemap_mark_dirty_out_of_range_index_is_a_noop() {
        let mut m = LayoutCacheMap::default();
        m.resize_to_tree(2);
        m.get_mut(0)
            .store_size(0, sizing_entry(size(1.0, 1.0), size(1.0, 1.0)));

        m.mark_dirty(99, &[]);
        m.mark_dirty(usize::MAX, &[]);

        // Guard clause hit: nothing was touched.
        assert!(!m.get(0).is_empty);
        assert_eq!(m.entries.len(), 2);
    }

    #[test]
    fn cachemap_mark_dirty_propagates_up_the_ancestor_chain() {
        // 0 <- 1 <- 2
        let tree = vec![plain(None), plain(Some(0)), plain(Some(1))];
        let mut m = LayoutCacheMap::default();
        m.resize_to_tree(3);
        for i in 0..3 {
            m.get_mut(i)
                .store_size(0, sizing_entry(size(1.0, 1.0), size(1.0, 1.0)));
        }

        m.mark_dirty(2, &tree);

        assert!(m.get(2).is_empty);
        assert!(m.get(1).is_empty);
        assert!(m.get(0).is_empty);
    }

    #[test]
    fn cachemap_mark_dirty_stops_at_the_first_dirty_ancestor() {
        // 0 (clean) <- 1 (already dirty) <- 2 (clean)
        let tree = vec![plain(None), plain(Some(0)), plain(Some(1))];
        let mut m = LayoutCacheMap::default();
        m.resize_to_tree(3);
        m.get_mut(0)
            .store_size(0, sizing_entry(size(1.0, 1.0), size(1.0, 1.0)));
        m.get_mut(2)
            .store_size(0, sizing_entry(size(1.0, 1.0), size(1.0, 1.0)));

        m.mark_dirty(2, &tree);

        assert!(m.get(2).is_empty);
        assert!(m.get(1).is_empty);
        // Early stop: the grandparent keeps its cached entry.
        assert!(!m.get(0).is_empty);
    }

    #[test]
    fn cachemap_mark_dirty_on_an_already_dirty_node_leaves_ancestors_alone() {
        let tree = vec![plain(None), plain(Some(0))];
        let mut m = LayoutCacheMap::default();
        m.resize_to_tree(2);
        m.get_mut(0)
            .store_size(0, sizing_entry(size(1.0, 1.0), size(1.0, 1.0)));
        // entry 1 is fresh → already dirty

        m.mark_dirty(1, &tree);

        assert!(m.get(1).is_empty);
        assert!(!m.get(0).is_empty);
    }

    #[test]
    fn cachemap_mark_dirty_terminates_on_cyclic_parent_links() {
        // A malformed tree (0 <-> 1, and 2 as its own parent) must not spin
        // forever: the `is_empty` early-stop breaks every cycle after one lap.
        let tree = vec![plain(Some(1)), plain(Some(0)), plain(Some(2))];
        let mut m = LayoutCacheMap::default();
        m.resize_to_tree(3);
        for i in 0..3 {
            m.get_mut(i)
                .store_size(0, sizing_entry(size(1.0, 1.0), size(1.0, 1.0)));
        }

        m.mark_dirty(0, &tree);
        assert!(m.get(0).is_empty);
        assert!(m.get(1).is_empty);

        m.mark_dirty(2, &tree);
        assert!(m.get(2).is_empty);
    }

    #[test]
    fn cachemap_mark_dirty_survives_a_tree_that_disagrees_with_the_cache() {
        let mut m = LayoutCacheMap::default();
        m.resize_to_tree(3);
        for i in 0..3 {
            m.get_mut(i)
                .store_size(0, sizing_entry(size(1.0, 1.0), size(1.0, 1.0)));
        }

        // Tree shorter than the cache: parent lookup returns None → stop.
        let short_tree = vec![plain(None)];
        m.mark_dirty(2, &short_tree);
        assert!(m.get(2).is_empty);
        assert!(!m.get(0).is_empty);

        // Parent index past the end of the cache: break, don't index-panic.
        let dangling = vec![plain(Some(usize::MAX)), plain(None), plain(None)];
        m.mark_dirty(0, &dangling);
        assert!(m.get(0).is_empty);
    }

    // ==================================================================
    // Solver3CacheMemoryReport / LayoutCache (getters)
    // ==================================================================

    #[test]
    fn memory_report_total_bytes_sums_every_field_exactly_once() {
        assert_eq!(Solver3CacheMemoryReport::default().total_bytes(), 0);

        // Distinct powers of two: a missing or double-counted field shows up as
        // a wrong total rather than an accidental coincidence.
        let r = Solver3CacheMemoryReport {
            tree_bytes: 1,
            tree_report: None,
            calculated_positions_bytes: 2,
            previous_positions_bytes: 4,
            scroll_ids_bytes: 8,
            scroll_id_to_node_id_bytes: 16,
            counters_bytes: 32,
            float_cache_bytes: 64,
            cache_map_bytes: 128,
            cached_display_list_bytes: 256,
            // Counters, not byte totals — must NOT contribute.
            cached_display_list_text_instances: 512,
            cached_display_list_items: 1024,
        };
        assert_eq!(r.total_bytes(), 511);
    }

    #[test]
    fn memory_report_of_a_default_cache_is_all_zero() {
        let cache = LayoutCache::default();
        let r = cache.memory_report();
        assert_eq!(r.total_bytes(), 0);
        assert_eq!(r.tree_bytes, 0);
        assert!(r.tree_report.is_none());
        assert_eq!(r.cache_map_bytes, 0);
        assert_eq!(r.cached_display_list_bytes, 0);
    }

    #[test]
    fn memory_report_accounts_for_populated_state() {
        let mut cache = LayoutCache::default();
        cache.cache_map.resize_to_tree(4);
        cache.calculated_positions = vec![pos(1.0, 2.0), pos(3.0, 4.0), pos(5.0, 6.0)];
        cache.previous_positions = vec![pos(0.0, 0.0)];
        cache.counters.insert((0, "list-item".to_string()), 7);
        cache.float_cache.insert(0, fc::FloatingContext::default());
        cache.scroll_ids.insert(LayoutNodeId::new(0), 42);
        cache.scroll_id_to_node_id.insert(42, NodeId::ZERO);
        // A DL with one Text item: the walk must count the item slot AND
        // its glyph Vec heap — this was a flat 2048 guess before, which
        // hid every cached DL's real size (the per-glyph copies).
        let glyphs = vec![azul_core::ui_solver::GlyphInstance::default(); 5];
        let glyph_heap = glyphs.capacity() * size_of::<azul_core::ui_solver::GlyphInstance>();
        let mut dl = DisplayList::default();
        dl.items
            .push(crate::solver3::display_list::DisplayListItem::Text {
                glyphs,
                font_hash: crate::text3::cache::FontHash::from_hash(1),
                font_size_px: 16.0,
                color: azul_css::props::basic::ColorU::BLACK,
                clip_rect: crate::solver3::display_list::WindowLogicalRect(LogicalRect::new(
                    pos(0.0, 0.0),
                    size(10.0, 10.0),
                )),
                source_node_index: None,
            });
        let dl_expected = dl.retained_bytes();
        cache.cached_display_list = Some((
            SubtreeHash(1),
            LogicalRect::new(pos(0.0, 0.0), size(10.0, 10.0)),
            0,
            0,
            0,
            std::sync::Arc::new(dl),
        ));

        let r = cache.memory_report();

        assert!(r.cache_map_bytes >= 4 * size_of::<NodeCache>());
        assert_eq!(
            r.calculated_positions_bytes,
            3 * size_of::<LogicalPosition>()
        );
        assert_eq!(r.previous_positions_bytes, size_of::<LogicalPosition>());
        assert!(r.counters_bytes >= "list-item".len());
        assert_eq!(r.float_cache_bytes, 256);
        assert_eq!(r.cached_display_list_bytes, dl_expected.0);
        assert_eq!(r.cached_display_list_text_instances, 5);
        assert_eq!(dl_expected.1, 5);
        assert_eq!(r.cached_display_list_items, 1);
        assert_eq!(dl_expected.2, 1);
        assert!(
            r.cached_display_list_bytes
                >= size_of::<crate::solver3::display_list::DisplayListItem>() + glyph_heap,
            "the Text item slot and its glyph heap must both be visible \
             (got {}, item {}, glyphs {glyph_heap})",
            r.cached_display_list_bytes,
            size_of::<crate::solver3::display_list::DisplayListItem>(),
        );
        assert_eq!(
            r.total_bytes(),
            r.tree_bytes
                + r.calculated_positions_bytes
                + r.previous_positions_bytes
                + r.scroll_ids_bytes
                + r.scroll_id_to_node_id_bytes
                + r.counters_bytes
                + r.float_cache_bytes
                + r.cache_map_bytes
                + r.cached_display_list_bytes
        );
    }

    /// The patch-damage LOG: a renderer that presented before two patched
    /// builds gets BOTH builds' damage (the first's vacated rect was the
    /// slider's thumb trail); a full emission in between hands authority
    /// back to the renderer's own diff; falling off the log means "repaint
    /// in full"; and nothing pending is nothing pending.
    #[test]
    fn pending_patch_damage_unions_every_patched_build_since_the_last_present() {
        let r = |x: f32| LogicalRect::new(pos(x, 0.0), size(16.0, 16.0));
        let mut cache = LayoutCache::default();
        assert_eq!(cache.pending_patch_damage(0), PendingPatchDamage::None);

        // The renderer presented against build 0. Then: a css patch (moves
        // the thumb 100 -> 120) and, in the same pass, the RefreshDom's
        // structure-preserved relayout (damages the caption + the thumb's
        // CURRENT rect only — it never saw 100).
        cache.record_patch_damage(vec![r(100.0), r(120.0)]);
        cache.record_patch_damage(vec![r(300.0), r(120.0)]);
        assert_eq!(
            cache.pending_patch_damage(0),
            PendingPatchDamage::Rects(vec![r(100.0), r(120.0), r(300.0), r(120.0)]),
            "the first build's vacated rect must survive the second build"
        );
        assert_eq!(
            cache.last_patch_damage,
            Some(vec![r(300.0), r(120.0)]),
            "the slot still reports the LAST build, for the frame report"
        );
        // Presented against build 2: nothing pending; a third patch is
        // pending on its own.
        assert_eq!(cache.pending_patch_damage(2), PendingPatchDamage::None);
        cache.record_patch_damage(vec![r(140.0)]);
        assert_eq!(
            cache.pending_patch_damage(2),
            PendingPatchDamage::Rects(vec![r(140.0)])
        );

        // A full emission retires the log: a renderer that has not seen it
        // must trust its own diff, and one that has sees nothing pending.
        cache.record_full_emission();
        assert_eq!(
            cache.pending_patch_damage(2),
            PendingPatchDamage::FullBuildSincePresent
        );
        assert_eq!(cache.pending_patch_damage(4), PendingPatchDamage::None);
        assert!(cache.last_patch_damage.is_none());
        assert!(!cache.last_build_was_patched);

        // Patches after the full emission chain from it.
        cache.record_patch_damage(vec![r(1.0)]);
        cache.record_patch_damage(vec![r(2.0)]);
        assert_eq!(
            cache.pending_patch_damage(4),
            PendingPatchDamage::Rects(vec![r(1.0), r(2.0)])
        );
        assert_eq!(
            cache.pending_patch_damage(5),
            PendingPatchDamage::Rects(vec![r(2.0)])
        );

        // Falling off the bounded log means the damage is unknown.
        for i in 0..40 {
            cache.record_patch_damage(vec![r(i as f32)]);
        }
        assert_eq!(cache.pending_patch_damage(4), PendingPatchDamage::Unknown);
        let seq = cache.build_seq;
        assert!(
            matches!(cache.pending_patch_damage(seq - 1), PendingPatchDamage::Rects(ref v) if v.len() == 1)
        );
    }

    #[test]
    fn reset_incremental_drops_reuse_state_keeps_the_rest_and_is_idempotent() {
        let mut cache = LayoutCache {
            tree: Some(build_tree(vec![plain(None)], warm_default(1), &[vec![]])),
            ..Default::default()
        };
        cache.cache_map.resize_to_tree(2);
        cache.cached_display_list = Some((
            SubtreeHash(9),
            LogicalRect::new(pos(0.0, 0.0), size(1.0, 1.0)),
            0,
            0,
            0,
            std::sync::Arc::new(DisplayList::default()),
        ));
        cache.prev_dom_ptr = 0xDEAD_BEEF;
        cache.counters.insert((0, "c".to_string()), 1);
        cache.float_cache.insert(0, fc::FloatingContext::default());
        // Not incremental-reuse state — must survive.
        cache.calculated_positions = vec![pos(1.0, 2.0)];
        cache.scroll_ids.insert(LayoutNodeId::new(0), 5);
        cache.viewport = Some(LogicalRect::new(pos(0.0, 0.0), size(800.0, 600.0)));

        cache.reset_incremental();

        assert!(cache.tree.is_none());
        assert!(cache.cache_map.entries.is_empty());
        assert!(cache.cached_display_list.is_none());
        assert_eq!(cache.prev_dom_ptr, 0);
        assert!(cache.counters.is_empty());
        assert!(cache.float_cache.is_empty());
        assert_eq!(cache.calculated_positions.len(), 1);
        assert_eq!(cache.scroll_ids.len(), 1);
        assert!(cache.viewport.is_some());

        // Idempotent: a second reset on the already-cold cache is a no-op.
        cache.reset_incremental();
        assert!(cache.tree.is_none());
        // Only the two retained fields (1 position + 1 scroll id) still cost bytes.
        assert_eq!(
            cache.memory_report().total_bytes(),
            size_of::<LogicalPosition>() + size_of::<usize>() + size_of::<u64>()
        );
    }

    // ==================================================================
    // ReconciliationResult (predicates)
    // ==================================================================

    #[test]
    fn reconciliation_result_default_is_clean() {
        let r = ReconciliationResult::default();
        assert!(r.is_clean());
        assert!(!r.needs_layout());
        assert!(!r.needs_paint_only());
    }

    #[test]
    fn reconciliation_result_predicates_hold_over_every_combination() {
        for intrinsic in [false, true] {
            for roots in [false, true] {
                for paint in [false, true] {
                    let mut r = ReconciliationResult::default();
                    if intrinsic {
                        r.intrinsic_dirty.insert(0);
                    }
                    if roots {
                        r.layout_roots.insert(usize::MAX);
                    }
                    if paint {
                        r.paint_dirty.insert(7);
                    }

                    let expect_layout = intrinsic || roots;
                    assert_eq!(r.needs_layout(), expect_layout);
                    assert_eq!(r.is_clean(), !intrinsic && !roots && !paint);
                    assert_eq!(r.needs_paint_only(), !expect_layout && paint);
                    // Invariants: clean ⇒ no work; layout and paint-only are exclusive.
                    assert!(!(r.is_clean() && (r.needs_layout() || r.needs_paint_only())));
                    assert!(!(r.needs_layout() && r.needs_paint_only()));
                }
            }
        }
    }

    // ==================================================================
    // to_overflow_behavior / style_text_align_to_fc (mapping tables)
    // ==================================================================

    #[test]
    fn to_overflow_behavior_maps_every_layout_overflow_variant() {
        assert_eq!(
            to_overflow_behavior(MultiValue::Exact(LayoutOverflow::Visible)),
            OverflowBehavior::Visible
        );
        assert_eq!(
            to_overflow_behavior(MultiValue::Exact(LayoutOverflow::Hidden)),
            OverflowBehavior::Hidden
        );
        assert_eq!(
            to_overflow_behavior(MultiValue::Exact(LayoutOverflow::Scroll)),
            OverflowBehavior::Scroll
        );
        assert_eq!(
            to_overflow_behavior(MultiValue::Exact(LayoutOverflow::Auto)),
            OverflowBehavior::Auto
        );
        // BEHAVIOUR PIN: `overflow: clip` is folded into Hidden, so the distinct
        // `OverflowBehavior::Clip` variant is never produced here. Clip differs
        // from hidden in CSS Overflow 3 (no scroll container, no scrollport), so
        // a `clip` box is currently treated as a (non-scrollable) hidden box.
        assert_eq!(
            to_overflow_behavior(MultiValue::Exact(LayoutOverflow::Clip)),
            OverflowBehavior::Hidden
        );
    }

    #[test]
    fn to_overflow_behavior_falls_back_to_the_initial_value() {
        // CSS Overflow 3: initial value is `visible`. Auto/Initial/Inherit here
        // are the *CSS-wide keyword* arms of MultiValue, not `overflow: auto`.
        for mv in [MultiValue::Auto, MultiValue::Initial, MultiValue::Inherit] {
            assert_eq!(to_overflow_behavior(mv), OverflowBehavior::Visible);
        }
    }

    #[test]
    fn overflow_auto_keyword_is_a_typed_value_not_a_css_wide_keyword() {
        // Regression guard: if `overflow: auto` were parsed as the generic
        // CSS-wide `auto` keyword it would arrive as MultiValue::Auto and
        // to_overflow_behavior would silently downgrade it to Visible — i.e. no
        // scrollbars at all. It must arrive as Exact(LayoutOverflow::Auto).
        let sd = styled(
            Dom::create_body().with_child(div_class("s")),
            ".s { overflow-x: auto; overflow-y: scroll; }",
        );
        let id = NodeId::new(1);
        let state = sd.styled_nodes.as_container()[id].styled_node_state;

        assert_eq!(
            to_overflow_behavior(get_overflow_x(&sd, id, &state)),
            OverflowBehavior::Auto
        );
        assert_eq!(
            to_overflow_behavior(get_overflow_y(&sd, id, &state)),
            OverflowBehavior::Scroll
        );
    }

    #[test]
    fn style_text_align_to_fc_maps_every_variant() {
        // fc::TextAlign has no PartialEq, so match on the variant.
        assert!(matches!(
            style_text_align_to_fc(StyleTextAlign::Start),
            fc::TextAlign::Start
        ));
        assert!(matches!(
            style_text_align_to_fc(StyleTextAlign::Left),
            fc::TextAlign::Start
        ));
        assert!(matches!(
            style_text_align_to_fc(StyleTextAlign::End),
            fc::TextAlign::End
        ));
        assert!(matches!(
            style_text_align_to_fc(StyleTextAlign::Right),
            fc::TextAlign::End
        ));
        assert!(matches!(
            style_text_align_to_fc(StyleTextAlign::Center),
            fc::TextAlign::Center
        ));
        assert!(matches!(
            style_text_align_to_fc(StyleTextAlign::Justify),
            fc::TextAlign::Justify
        ));
    }

    // ==================================================================
    // should_use_content_height (predicate)
    // ==================================================================

    #[test]
    fn should_use_content_height_for_css_wide_keywords_and_auto() {
        assert!(should_use_content_height(&MultiValue::Auto));
        assert!(should_use_content_height(&MultiValue::Initial));
        assert!(should_use_content_height(&MultiValue::Inherit));
        assert!(should_use_content_height(&MultiValue::Exact(
            LayoutHeight::Auto
        )));
    }

    #[test]
    fn should_use_content_height_is_false_for_definite_lengths() {
        for pv in [
            PixelValue::px(100.0),
            PixelValue::px(-10.0),
            PixelValue::percent(50.0),
            PixelValue::percent(0.0),
            PixelValue::em(2.0),
            PixelValue::rem(2.0),
        ] {
            assert!(
                !should_use_content_height(&MultiValue::Exact(LayoutHeight::Px(pv))),
                "expected a definite height for {pv:?}"
            );
        }
    }

    #[test]
    fn should_use_content_height_treats_zero_px_as_content_based() {
        // BEHAVIOUR PIN: `height: 0px` is indistinguishable from `auto` here, so
        // an explicitly zero-height box falls back to content sizing.
        assert!(should_use_content_height(&MultiValue::Exact(
            LayoutHeight::Px(PixelValue::zero())
        )));
        assert!(should_use_content_height(&MultiValue::Exact(
            LayoutHeight::Px(PixelValue::px(0.0))
        )));
        // ...but `height: 0%` is NOT (its metric is Percent, not Px).
        assert!(!should_use_content_height(&MultiValue::Exact(
            LayoutHeight::Px(PixelValue::percent(0.0))
        )));
    }

    #[test]
    fn should_use_content_height_treats_non_px_metrics_as_content_based() {
        // BEHAVIOUR PIN (suspected bug): only Px/Percent/Em/Rem are recognised as
        // definite. Every other metric — pt, vh, vw, cm, in, mm — is reported as
        // content-based, so `height: 100vh` behaves like a *minimum* height (see
        // apply_content_based_height, which takes max(used, content)) instead of
        // a definite one.
        for metric in [
            SizeMetric::Pt,
            SizeMetric::Vh,
            SizeMetric::Vw,
            SizeMetric::Cm,
            SizeMetric::Mm,
            SizeMetric::In,
        ] {
            let pv = PixelValue::from_metric(metric, 100.0);
            assert!(
                should_use_content_height(&MultiValue::Exact(LayoutHeight::Px(pv))),
                "{metric:?} is currently treated as content-based"
            );
        }
    }

    #[test]
    fn should_use_content_height_for_intrinsic_keywords_but_not_calc() {
        assert!(should_use_content_height(&MultiValue::Exact(
            LayoutHeight::MinContent
        )));
        assert!(should_use_content_height(&MultiValue::Exact(
            LayoutHeight::MaxContent
        )));
        assert!(should_use_content_height(&MultiValue::Exact(
            LayoutHeight::FitContent(PixelValue::px(10.0))
        )));
        // calc() resolves to a definite value.
        assert!(!should_use_content_height(&MultiValue::Exact(
            LayoutHeight::Calc(CalcAstItemVec::from_vec(Vec::new()))
        )));
    }

    // ==================================================================
    // apply_content_based_height (numeric)
    // ==================================================================

    fn one_node_tree_with(bp: &ResolvedBoxProps) -> LayoutTree {
        build_tree(
            vec![hot(None, None, Some(size(100.0, 100.0)), bp)],
            warm_default(1),
            &[vec![]],
        )
    }

    #[test]
    fn apply_content_based_height_keeps_the_larger_of_min_height_and_content() {
        let bp = box_props(
            edges(0.0, 0.0, 0.0, 0.0),
            edges(0.0, 0.0, 0.0, 0.0),
            edges(10.0, 0.0, 10.0, 0.0), // padding: 20px on the block axis
        );
        let tree = one_node_tree_with(&bp);
        let wm = LayoutWritingMode::HorizontalTb;

        // Content (50 + 20 padding = 70) is smaller than the min-height-constrained
        // 100 → the Phase-1 size wins (CSS 2.2 § 10.7).
        let out = apply_content_based_height(size(100.0, 100.0), size(0.0, 50.0), &tree, 0, wm)
            .expect("valid node");
        assert_eq!(out, size(100.0, 100.0));

        // Content wins when it is taller.
        let out = apply_content_based_height(size(100.0, 40.0), size(0.0, 50.0), &tree, 0, wm)
            .expect("valid node");
        assert_eq!(out, size(100.0, 70.0));
    }

    #[test]
    fn apply_content_based_height_at_zero_and_in_vertical_writing_modes() {
        let tree = one_node_tree_with(&zero_box_props());

        let out = apply_content_based_height(
            size(0.0, 0.0),
            size(0.0, 0.0),
            &tree,
            0,
            LayoutWritingMode::HorizontalTb,
        )
        .expect("valid node");
        assert_eq!(out, size(0.0, 0.0));

        // Vertical writing mode: the main axis is the *width*.
        let out = apply_content_based_height(
            size(10.0, 80.0),
            size(50.0, 0.0),
            &tree,
            0,
            LayoutWritingMode::VerticalRl,
        )
        .expect("valid node");
        assert_eq!(out, size(50.0, 80.0));
    }

    #[test]
    fn apply_content_based_height_does_not_propagate_nan() {
        let tree = one_node_tree_with(&zero_box_props());
        let wm = LayoutWritingMode::HorizontalTb;

        // f32::max ignores a NaN operand, so a NaN content size cannot poison the
        // used size — the Phase-1 height survives.
        let out = apply_content_based_height(size(100.0, 40.0), size(0.0, f32::NAN), &tree, 0, wm)
            .expect("valid node");
        assert!(!out.height.is_nan());
        assert_eq!(out.height, 40.0);

        // ...and symmetrically, a NaN used size is replaced by the content size.
        let out = apply_content_based_height(size(100.0, f32::NAN), size(0.0, 50.0), &tree, 0, wm)
            .expect("valid node");
        assert!(!out.height.is_nan());
        assert_eq!(out.height, 50.0);
    }

    #[test]
    fn apply_content_based_height_saturates_at_infinity_and_rejects_bad_indices() {
        let tree = one_node_tree_with(&zero_box_props());
        let wm = LayoutWritingMode::HorizontalTb;

        let out =
            apply_content_based_height(size(10.0, 10.0), size(0.0, f32::INFINITY), &tree, 0, wm)
                .expect("valid node");
        assert!(out.height.is_infinite() && out.height.is_sign_positive());

        // A huge finite content size stays finite (no overflow panic).
        let out = apply_content_based_height(size(10.0, 10.0), size(0.0, f32::MAX), &tree, 0, wm)
            .expect("valid node");
        assert_eq!(out.height, f32::MAX);

        // Out-of-range node → Err, not a panic.
        assert!(apply_content_based_height(size(1.0, 1.0), size(1.0, 1.0), &tree, 1, wm).is_err());
        assert!(
            apply_content_based_height(size(1.0, 1.0), size(1.0, 1.0), &tree, usize::MAX, wm)
                .is_err()
        );
    }

    // ==================================================================
    // calculate_subtree_hash (numeric / round-trip-ish)
    // ==================================================================

    #[test]
    fn subtree_hash_is_deterministic() {
        let a = calculate_subtree_hash(42, &[1, 2, 3]);
        let b = calculate_subtree_hash(42, &[1, 2, 3]);
        assert_eq!(a, b);
        assert_eq!(a, calculate_subtree_hash(42, &[1, 2, 3]));
    }

    #[test]
    fn subtree_hash_depends_on_child_order_and_arity() {
        let base = calculate_subtree_hash(1, &[10, 20]);
        assert_ne!(
            base,
            calculate_subtree_hash(1, &[20, 10]),
            "order must matter"
        );
        assert_ne!(
            base,
            calculate_subtree_hash(1, &[10, 20, 30]),
            "arity must matter"
        );
        assert_ne!(
            base,
            calculate_subtree_hash(1, &[10]),
            "a dropped child must matter"
        );
        assert_ne!(
            base,
            calculate_subtree_hash(2, &[10, 20]),
            "self hash must matter"
        );
    }

    #[test]
    fn subtree_hash_distinguishes_no_children_from_a_zero_child() {
        // Length is folded in (slice Hash writes the length), so an empty child
        // list is not confusable with a single 0-hash child.
        assert_ne!(
            calculate_subtree_hash(0, &[]),
            calculate_subtree_hash(0, &[0])
        );
        assert_ne!(
            calculate_subtree_hash(0, &[0]),
            calculate_subtree_hash(0, &[0, 0])
        );
    }

    #[test]
    fn subtree_hash_does_not_swap_self_hash_and_child_hash() {
        assert_ne!(
            calculate_subtree_hash(7, &[9]),
            calculate_subtree_hash(9, &[7])
        );
    }

    #[test]
    fn subtree_hash_handles_extremes_and_large_child_lists() {
        let extremes = calculate_subtree_hash(u64::MAX, &[u64::MAX, 0, u64::MAX]);
        assert_eq!(
            extremes,
            calculate_subtree_hash(u64::MAX, &[u64::MAX, 0, u64::MAX])
        );
        assert_ne!(extremes, calculate_subtree_hash(0, &[0, 0, 0]));

        // 10k children: no overflow, no panic, still deterministic.
        let many: Vec<u64> = (0..10_000).collect();
        assert_eq!(
            calculate_subtree_hash(1, &many),
            calculate_subtree_hash(1, &many)
        );
    }

    // ==================================================================
    // calculate_content_box_pos (numeric)
    // ==================================================================

    #[test]
    fn content_box_pos_adds_border_and_padding_but_not_margin() {
        let bp = box_props(
            edges(999.0, 999.0, 999.0, 999.0), // margin must be ignored
            edges(2.0, 0.0, 0.0, 3.0),         // border top/left
            edges(5.0, 0.0, 0.0, 7.0),         // padding top/left
        );
        let out = calculate_content_box_pos(pos(10.0, 20.0), &bp);
        assert_eq!(out, pos(10.0 + 3.0 + 7.0, 20.0 + 2.0 + 5.0));

        // Zero box props → identity.
        assert_eq!(
            calculate_content_box_pos(pos(4.0, 5.0), &zero_box_props()),
            pos(4.0, 5.0)
        );
    }

    #[test]
    fn content_box_pos_with_negative_edges_shifts_backwards() {
        let bp = box_props(
            edges(0.0, 0.0, 0.0, 0.0),
            edges(-1.0, 0.0, 0.0, -2.0),
            edges(-3.0, 0.0, 0.0, -4.0),
        );
        assert_eq!(
            calculate_content_box_pos(pos(0.0, 0.0), &bp),
            pos(-6.0, -4.0)
        );
    }

    #[test]
    fn content_box_pos_with_non_finite_edges_is_deterministic() {
        let nan_bp = box_props(
            edges(0.0, 0.0, 0.0, 0.0),
            edges(f32::NAN, 0.0, 0.0, f32::NAN),
            edges(0.0, 0.0, 0.0, 0.0),
        );
        let out = calculate_content_box_pos(pos(1.0, 1.0), &nan_bp);
        assert!(
            out.x.is_nan() && out.y.is_nan(),
            "NaN edges propagate, no panic"
        );

        // +inf border with a -inf containing block → NaN, still no panic.
        let inf_bp = box_props(
            edges(0.0, 0.0, 0.0, 0.0),
            edges(f32::INFINITY, 0.0, 0.0, f32::INFINITY),
            edges(0.0, 0.0, 0.0, 0.0),
        );
        let out = calculate_content_box_pos(pos(f32::NEG_INFINITY, f32::NEG_INFINITY), &inf_bp);
        assert!(out.x.is_nan() && out.y.is_nan());

        // Saturation rather than wrap-around at the top of the f32 range.
        let max_bp = box_props(
            edges(0.0, 0.0, 0.0, 0.0),
            edges(f32::MAX, 0.0, 0.0, f32::MAX),
            edges(f32::MAX, 0.0, 0.0, f32::MAX),
        );
        let out = calculate_content_box_pos(pos(f32::MAX, f32::MAX), &max_bp);
        assert!(out.x.is_infinite() && out.x.is_sign_positive());
        assert!(out.y.is_infinite() && out.y.is_sign_positive());
    }

    // ==================================================================
    // check_scrollbar_change (numeric / predicate)
    // ==================================================================

    fn scrollbars(h: bool, v: bool, w: f32) -> ScrollbarRequirements {
        ScrollbarRequirements {
            needs_horizontal: h,
            needs_vertical: v,
            scrollbar_width: w,
            scrollbar_height: w,
            visual_width_px: w,
        }
    }

    fn tree_with_scrollbar_info(info: Option<ScrollbarRequirements>) -> LayoutTree {
        let mut warm = warm_default(1);
        warm[0].scrollbar_info = info;
        build_tree(vec![plain(None)], warm, &[vec![]])
    }

    #[test]
    fn check_scrollbar_change_skip_flag_short_circuits_everything() {
        let tree = tree_with_scrollbar_info(Some(scrollbars(false, false, 0.0)));
        // Even a full add of both scrollbars is suppressed by the skip flag.
        assert!(!check_scrollbar_change(
            &tree,
            0,
            &scrollbars(true, true, 16.0),
            true
        ));
    }

    #[test]
    fn check_scrollbar_change_out_of_range_node_is_false() {
        let tree = tree_with_scrollbar_info(None);
        assert!(!check_scrollbar_change(
            &tree,
            1,
            &scrollbars(true, true, 16.0),
            false
        ));
        assert!(!check_scrollbar_change(
            &tree,
            usize::MAX,
            &scrollbars(true, true, 16.0),
            false
        ));
    }

    #[test]
    fn check_scrollbar_change_without_previous_info_uses_needs_reflow() {
        let tree = tree_with_scrollbar_info(None);
        // No reserved space → nothing to reflow for.
        assert!(!check_scrollbar_change(
            &tree,
            0,
            &scrollbars(true, true, 0.0),
            false
        ));
        // Reserved space → reflow.
        assert!(check_scrollbar_change(
            &tree,
            0,
            &scrollbars(false, false, 16.0),
            false
        ));
        // NaN reserved width: `NaN > 0.0` is false → deterministic `false`, no panic.
        assert!(!check_scrollbar_change(
            &tree,
            0,
            &scrollbars(true, true, f32::NAN),
            false
        ));
    }

    #[test]
    fn check_scrollbar_change_detects_both_addition_and_removal() {
        let had_vertical = tree_with_scrollbar_info(Some(scrollbars(false, true, 16.0)));
        // Removal (vertical true → false) must be detected, not suppressed.
        assert!(check_scrollbar_change(
            &had_vertical,
            0,
            &scrollbars(false, false, 0.0),
            false
        ));
        // Addition of the horizontal bar.
        assert!(check_scrollbar_change(
            &had_vertical,
            0,
            &scrollbars(true, true, 16.0),
            false
        ));
        // No change at all.
        assert!(!check_scrollbar_change(
            &had_vertical,
            0,
            &scrollbars(false, true, 16.0),
            false
        ));
    }

    #[test]
    fn check_scrollbar_change_ignores_width_only_changes() {
        // BEHAVIOUR PIN: only the needs_horizontal/needs_vertical booleans are
        // compared. A scrollbar that keeps existing but changes reserved width
        // (e.g. a restyle from a classic to a thin scrollbar) does not trigger a
        // reflow here.
        let tree = tree_with_scrollbar_info(Some(scrollbars(false, true, 16.0)));
        assert!(!check_scrollbar_change(
            &tree,
            0,
            &scrollbars(false, true, 4.0),
            false
        ));
    }

    #[test]
    fn check_scrollbar_change_ignores_a_flip_between_two_zero_reservation_states() {
        // Overlay scrollbars reserve nothing, so a bar appearing or vanishing
        // cannot change the available space a reflow would recompute. Same for
        // the post-layout `apply_virtual_scroll_necessity` amendment, which
        // raises a flag without reserving a gutter — without this, an
        // `overflow: auto` VirtualView asked for a full extra layout pass on
        // every single pass, forever.
        let overlay_bar = tree_with_scrollbar_info(Some(scrollbars(false, true, 0.0)));
        assert!(!check_scrollbar_change(
            &overlay_bar,
            0,
            &scrollbars(false, false, 0.0),
            false
        ));
        let no_bar = tree_with_scrollbar_info(Some(scrollbars(false, false, 0.0)));
        assert!(!check_scrollbar_change(
            &no_bar,
            0,
            &scrollbars(false, true, 0.0),
            false
        ));

        // A space-reserving bar on EITHER side still forces the reflow.
        assert!(check_scrollbar_change(
            &no_bar,
            0,
            &scrollbars(false, true, 16.0),
            false
        ));
        let classic_bar = tree_with_scrollbar_info(Some(scrollbars(false, true, 16.0)));
        assert!(check_scrollbar_change(
            &classic_bar,
            0,
            &scrollbars(false, false, 0.0),
            false
        ));
    }

    // ==================================================================
    // apply_virtual_scroll_necessity
    // ==================================================================

    /// `body(0) > node(1)`, where node 1 carries `.vv` and the given node type.
    fn dom_with_styled_child(node_type: NodeType, css_str: &str) -> StyledDom {
        styled(
            Dom::create_body().with_child(
                Dom::create_node(node_type)
                    .with_ids_and_classes(vec![IdOrClass::Class("vv".into())].into()),
            ),
            css_str,
        )
    }

    fn no_scrollbars() -> ScrollbarRequirements {
        scrollbars(false, false, 0.0)
    }

    #[test]
    fn apply_virtual_scroll_necessity_raises_the_axis_the_virtual_document_overflows() {
        let dom = dom_with_styled_child(
            NodeType::VirtualView,
            ".vv { overflow-x: hidden; overflow-y: auto; }",
        );
        let mut info = no_scrollbars();
        assert!(apply_virtual_scroll_necessity(
            &dom,
            NodeId::new(1),
            size(100.0, 1000.0),
            size(100.0, 100.0),
            &mut info,
        ));
        assert!(info.needs_vertical);
        assert!(!info.needs_horizontal, "the hidden axis never gains a bar");
        // Layout has already run: no gutter is reserved after the fact.
        assert_eq!(info.scrollbar_width, 0.0);
        assert_eq!(info.scrollbar_height, 0.0);
    }

    #[test]
    fn apply_virtual_scroll_necessity_ignores_everything_that_is_not_a_virtual_view() {
        // Same CSS, same overflowing virtual size — a plain div's necessity is
        // layout's business and must not be second-guessed here.
        let dom = dom_with_styled_child(NodeType::Div, ".vv { overflow: auto; }");
        let mut info = no_scrollbars();
        assert!(!apply_virtual_scroll_necessity(
            &dom,
            NodeId::new(1),
            size(1000.0, 1000.0),
            size(100.0, 100.0),
            &mut info,
        ));
        assert!(!info.needs_vertical && !info.needs_horizontal);

        // An out-of-range node is inert rather than a panic.
        let mut info = no_scrollbars();
        assert!(!apply_virtual_scroll_necessity(
            &dom,
            NodeId::new(9_999),
            size(1000.0, 1000.0),
            size(100.0, 100.0),
            &mut info,
        ));
    }

    #[test]
    fn apply_virtual_scroll_necessity_honours_the_one_pixel_epsilon_and_the_overflow_value() {
        let auto = dom_with_styled_child(NodeType::VirtualView, ".vv { overflow: auto; }");
        // Exactly at the boundary: 101 is NOT > 100 + 1 — same tolerance as
        // check_scrollbar_necessity, so sub-pixel noise cannot flicker a bar.
        let mut info = no_scrollbars();
        assert!(!apply_virtual_scroll_necessity(
            &auto,
            NodeId::new(1),
            size(101.0, 101.0),
            size(100.0, 100.0),
            &mut info,
        ));
        // One pixel past it raises both axes.
        let mut info = no_scrollbars();
        assert!(apply_virtual_scroll_necessity(
            &auto,
            NodeId::new(1),
            size(102.0, 102.0),
            size(100.0, 100.0),
            &mut info,
        ));
        assert!(info.needs_horizontal && info.needs_vertical);

        // `hidden` is a scroll container but not a user-scrollable one: no bar,
        // however large the virtual document is.
        let hidden = dom_with_styled_child(NodeType::VirtualView, ".vv { overflow: hidden; }");
        let mut info = no_scrollbars();
        assert!(!apply_virtual_scroll_necessity(
            &hidden,
            NodeId::new(1),
            size(9999.0, 9999.0),
            size(100.0, 100.0),
            &mut info,
        ));
        assert!(!info.needs_horizontal && !info.needs_vertical);
    }

    #[test]
    fn apply_virtual_scroll_necessity_never_lowers_a_flag_layout_already_raised() {
        // `overflow: scroll` sets needs_* unconditionally at layout time (and
        // reserves the gutter). A virtual size that fits must not undo that.
        let dom = dom_with_styled_child(NodeType::VirtualView, ".vv { overflow-y: scroll; }");
        let mut info = scrollbars(false, true, 16.0);
        assert!(!apply_virtual_scroll_necessity(
            &dom,
            NodeId::new(1),
            size(100.0, 10.0),
            size(100.0, 100.0),
            &mut info,
        ));
        assert!(info.needs_vertical, "the reserved bar survives");
        assert_eq!(
            info.scrollbar_width, 16.0,
            "and keeps its layout reservation"
        );
    }

    // ==================================================================
    // shift_subtree_position (numeric)
    // ==================================================================

    /// 0 -> [1, 2]; 1 -> [3]
    fn shift_fixture() -> (LayoutTree, PositionVec) {
        let tree = build_tree(
            vec![plain(None), plain(Some(0)), plain(Some(0)), plain(Some(1))],
            warm_default(4),
            &[vec![1, 2], vec![3], vec![], vec![]],
        );
        let positions = vec![
            pos(0.0, 0.0),
            pos(10.0, 10.0),
            pos(20.0, 20.0),
            pos(30.0, 30.0),
        ];
        (tree, positions)
    }

    #[test]
    fn shift_subtree_position_zero_delta_is_identity() {
        let (tree, mut positions) = shift_fixture();
        let before = positions.clone();
        shift_subtree_position(0, pos(0.0, 0.0), &tree, &mut positions);
        assert_eq!(positions, before);
    }

    #[test]
    fn shift_subtree_position_moves_the_subtree_and_leaves_siblings_alone() {
        let (tree, mut positions) = shift_fixture();
        shift_subtree_position(1, pos(5.0, -3.0), &tree, &mut positions);

        assert_eq!(positions[0], pos(0.0, 0.0), "parent untouched");
        assert_eq!(positions[1], pos(15.0, 7.0), "shifted node");
        assert_eq!(positions[2], pos(20.0, 20.0), "sibling untouched");
        assert_eq!(positions[3], pos(35.0, 27.0), "descendant follows");
    }

    #[test]
    fn shift_subtree_position_out_of_range_node_is_a_noop() {
        let (tree, mut positions) = shift_fixture();
        let before = positions.clone();
        shift_subtree_position(99, pos(5.0, 5.0), &tree, &mut positions);
        shift_subtree_position(usize::MAX, pos(5.0, 5.0), &tree, &mut positions);
        assert_eq!(positions, before);
    }

    #[test]
    fn shift_subtree_position_skips_nodes_without_a_stored_position() {
        let (tree, _) = shift_fixture();
        // Only node 0 has a position; 1..3 are missing from the vec entirely.
        let mut positions = vec![pos(1.0, 1.0)];
        shift_subtree_position(0, pos(2.0, 2.0), &tree, &mut positions);
        assert_eq!(positions.len(), 1, "the vec is not grown by the shift");
        assert_eq!(positions[0], pos(3.0, 3.0));
    }

    #[test]
    fn shift_subtree_position_leaves_the_unset_sentinel_unset() {
        // POSITION_UNSET is f32::MIN; adding a small delta to it is a no-op at
        // f32 precision, so an un-positioned node stays "unset" after a shift
        // instead of turning into a bogus near-MIN coordinate.
        let (tree, _) = shift_fixture();
        let mut positions = vec![
            pos(0.0, 0.0),
            POSITION_UNSET,
            POSITION_UNSET,
            POSITION_UNSET,
        ];
        shift_subtree_position(0, pos(7.0, 9.0), &tree, &mut positions);

        assert_eq!(pos_get(&positions, 0), Some(pos(7.0, 9.0)));
        assert!(pos_get(&positions, 1).is_none());
        assert!(pos_get(&positions, 3).is_none());
    }

    #[test]
    fn shift_subtree_position_nan_delta_is_confined_to_the_subtree() {
        let (tree, mut positions) = shift_fixture();
        shift_subtree_position(1, pos(f32::NAN, f32::NAN), &tree, &mut positions);

        assert!(positions[1].x.is_nan() && positions[1].y.is_nan());
        assert!(positions[3].x.is_nan(), "NaN reaches the descendant");
        assert!(!positions[2].x.is_nan(), "the sibling is untouched");
        assert_eq!(positions[0], pos(0.0, 0.0));
    }

    #[test]
    fn shift_subtree_position_walks_a_deep_chain_without_blowing_the_stack() {
        const DEPTH: usize = 500;
        let mut nodes = vec![plain(None)];
        let mut child_lists: Vec<Vec<usize>> = vec![vec![1]];
        for i in 1..DEPTH {
            nodes.push(plain(Some(i - 1)));
            child_lists.push(if i + 1 < DEPTH { vec![i + 1] } else { vec![] });
        }
        let tree = build_tree(nodes, warm_default(DEPTH), &child_lists);
        let mut positions = vec![pos(0.0, 0.0); DEPTH];

        shift_subtree_position(0, pos(1.0, 2.0), &tree, &mut positions);

        assert_eq!(positions[0], pos(1.0, 2.0));
        assert_eq!(positions[DEPTH - 1], pos(1.0, 2.0));
    }

    // ==================================================================
    // position_bfc_child_descendants / position_flex_child_descendants
    // ==================================================================

    #[test]
    fn position_bfc_child_descendants_out_of_range_node_is_a_noop() {
        let (tree, mut positions) = shift_fixture();
        let before = positions.clone();
        position_bfc_child_descendants(&tree, 99, pos(1.0, 1.0), &mut positions);
        assert_eq!(positions, before);
    }

    #[test]
    fn position_bfc_child_descendants_converts_relative_to_absolute() {
        // 0 -> [1] -> [2]; node 1 has a 2px border + 3px padding on top/left.
        let bp = box_props(
            edges(0.0, 0.0, 0.0, 0.0),
            edges(2.0, 0.0, 0.0, 2.0),
            edges(3.0, 0.0, 0.0, 3.0),
        );
        let mut warm = warm_default(3);
        warm[1].relative_position = Some(pos(10.0, 20.0));
        warm[2].relative_position = Some(pos(1.0, 1.0));
        let tree = build_tree(
            vec![
                plain(None),
                hot(Some(0), None, Some(size(50.0, 50.0)), &bp),
                plain(Some(1)),
            ],
            warm,
            &[vec![1], vec![2], vec![]],
        );

        let mut positions: PositionVec = Vec::new();
        position_bfc_child_descendants(&tree, 0, pos(100.0, 200.0), &mut positions);

        // child: content-box origin + relative
        assert_eq!(pos_get(&positions, 1), Some(pos(110.0, 220.0)));
        // grandchild: child's own content box (abs + border + padding) + relative
        assert_eq!(
            pos_get(&positions, 2),
            Some(pos(110.0 + 5.0 + 1.0, 220.0 + 5.0 + 1.0))
        );
    }

    #[test]
    fn position_bfc_child_descendants_defaults_a_missing_relative_position_to_the_origin() {
        let tree = build_tree(
            vec![plain(None), plain(Some(0))],
            warm_default(2), // relative_position: None
            &[vec![1], vec![]],
        );
        let mut positions: PositionVec = Vec::new();
        position_bfc_child_descendants(&tree, 0, pos(7.0, 8.0), &mut positions);
        assert_eq!(pos_get(&positions, 1), Some(pos(7.0, 8.0)));
    }

    #[test]
    fn position_flex_child_descendants_rejects_a_dangling_child_index() {
        // children_arena points at a node that does not exist → Err, not a panic.
        let mut tree = build_tree(vec![plain(None)], warm_default(1), &[vec![99]]);
        let mut positions: PositionVec = Vec::new();
        let r = position_flex_child_descendants(
            &mut tree,
            0,
            pos(0.0, 0.0),
            size(100.0, 100.0),
            &mut positions,
        );
        assert!(r.is_err());
    }

    #[test]
    fn position_flex_child_descendants_positions_children_and_grandchildren() {
        let mut warm = warm_default(3);
        warm[1].relative_position = Some(pos(5.0, 5.0));
        warm[2].relative_position = Some(pos(2.0, 2.0));
        let mut tree = build_tree(
            vec![plain(None), plain(Some(0)), plain(Some(1))],
            warm,
            &[vec![1], vec![2], vec![]],
        );

        let mut positions: PositionVec = Vec::new();
        position_flex_child_descendants(
            &mut tree,
            0,
            pos(50.0, 60.0),
            size(100.0, 100.0),
            &mut positions,
        )
        .expect("well-formed tree");

        assert_eq!(pos_get(&positions, 1), Some(pos(55.0, 65.0)));
        assert_eq!(pos_get(&positions, 2), Some(pos(57.0, 67.0)));
    }

    // ==================================================================
    // collect_children_dom_ids / layout_relevant_child_count
    // ==================================================================

    #[test]
    fn collect_children_dom_ids_skips_display_none_children() {
        // body(0) > [ div(1), div.hide(2), div(3) ]
        let sd = styled(
            Dom::create_body()
                .with_child(Dom::create_div())
                .with_child(div_class("hide"))
                .with_child(Dom::create_div()),
            ".hide { display: none; }",
        );

        let children = collect_children_dom_ids(&sd, NodeId::ZERO);
        assert_eq!(children, vec![NodeId::new(1), NodeId::new(3)]);
    }

    #[test]
    fn collect_children_dom_ids_for_leaves_and_unknown_parents_is_empty() {
        let sd = styled(Dom::create_body().with_child(Dom::create_div()), "");

        // A leaf has no children.
        assert!(collect_children_dom_ids(&sd, NodeId::new(1)).is_empty());
        // An id past the end of the hierarchy must not panic.
        assert!(collect_children_dom_ids(&sd, NodeId::new(999)).is_empty());
        assert!(collect_children_dom_ids(&sd, NodeId::new(usize::MAX)).is_empty());
    }

    #[test]
    fn layout_relevant_child_count_counts_only_boxes_the_builder_would_emit() {
        // body(0) > [ div(1), text " "(2), div.hide(3) ]
        let sd = styled(
            Dom::create_body()
                .with_child(Dom::create_div())
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(" "))
                .with_child(div_class("hide")),
            ".hide { display: none; }",
        );
        let children = [NodeId::new(1), NodeId::new(2), NodeId::new(3)];

        // display:none is dropped, and the whitespace-only inline run between
        // block siblings collapses → only the first div survives.
        assert_eq!(layout_relevant_child_count(&sd, &children, NodeId::ZERO), 1);
        // Empty input → 0, no panic.
        assert_eq!(layout_relevant_child_count(&sd, &[], NodeId::ZERO), 0);
    }

    // ==================================================================
    // is_whitespace_only_inline_run (predicate)
    // ==================================================================

    #[test]
    fn is_whitespace_only_inline_run_empty_run_is_true() {
        let sd = whitespace_dom("");
        assert!(is_whitespace_only_inline_run(&sd, &[], NodeId::new(1)));
    }

    #[test]
    fn is_whitespace_only_inline_run_detects_collapsible_whitespace_text() {
        let sd = whitespace_dom("");
        // " \n\t" — only ASCII whitespace.
        assert!(is_whitespace_only_inline_run(
            &sd,
            &[(0, NodeId::new(2))],
            NodeId::new(1)
        ));
        // "hi" — real text.
        assert!(!is_whitespace_only_inline_run(
            &sd,
            &[(1, NodeId::new(3))],
            NodeId::new(1)
        ));
        // A mixed run is not whitespace-only.
        assert!(!is_whitespace_only_inline_run(
            &sd,
            &[(0, NodeId::new(2)), (1, NodeId::new(3))],
            NodeId::new(1)
        ));
    }

    #[test]
    fn is_whitespace_only_inline_run_rejects_elements_and_non_ascii_spaces() {
        let sd = whitespace_dom("");
        // A <div> in the run is not a text node → wrapper required.
        assert!(!is_whitespace_only_inline_run(
            &sd,
            &[(2, NodeId::new(4))],
            NodeId::new(1)
        ));
        // U+00A0 NO-BREAK SPACE is *not* collapsible whitespace in CSS, and the
        // ASCII-only char set correctly treats it as real content.
        assert!(!is_whitespace_only_inline_run(
            &sd,
            &[(3, NodeId::new(5))],
            NodeId::new(1)
        ));
    }

    #[test]
    fn is_whitespace_only_inline_run_respects_white_space_pre() {
        // With `white-space: pre` on the parent, whitespace is significant and the
        // anonymous IFC wrapper must still be created.
        let sd = whitespace_dom(".p { white-space: pre; }");
        assert!(!is_whitespace_only_inline_run(
            &sd,
            &[(0, NodeId::new(2))],
            NodeId::new(1)
        ));

        // Sanity: the same run collapses without the `pre`.
        let sd = whitespace_dom(".p { white-space: normal; }");
        assert!(is_whitespace_only_inline_run(
            &sd,
            &[(0, NodeId::new(2))],
            NodeId::new(1)
        ));
    }

    // ==================================================================
    // reposition_clean_subtrees / reposition_block_flow_siblings
    // ==================================================================

    #[test]
    fn reposition_clean_subtrees_ignores_empty_and_dangling_roots() {
        let sd = styled(Dom::create_body().with_child(Dom::create_div()), "");
        let tree = build_tree(
            vec![
                hot(
                    None,
                    Some(NodeId::ZERO),
                    Some(size(100.0, 100.0)),
                    &zero_box_props(),
                ),
                plain(Some(0)),
            ],
            warm_default(2),
            &[vec![1], vec![]],
        );
        let mut positions = vec![pos(0.0, 0.0), pos(1.0, 1.0)];
        let before = positions.clone();

        // No dirty roots → nothing to reposition.
        reposition_clean_subtrees(&sd, &tree, &BTreeSet::new(), &mut positions);
        assert_eq!(positions, before);

        // A layout root that isn't in the tree must not panic.
        let mut roots = BTreeSet::new();
        roots.insert(99);
        roots.insert(usize::MAX);
        reposition_clean_subtrees(&sd, &tree, &roots, &mut positions);
        assert_eq!(positions, before);
    }

    #[test]
    fn reposition_clean_subtrees_skips_flex_parents() {
        let sd = styled(Dom::create_body().with_child(Dom::create_div()), "");
        let mut nodes = vec![
            hot(
                None,
                Some(NodeId::ZERO),
                Some(size(100.0, 100.0)),
                &zero_box_props(),
            ),
            plain(Some(0)),
        ];
        nodes[0].formatting_context = FormattingContext::Flex;
        let tree = build_tree(nodes, warm_default(2), &[vec![1], vec![]]);

        let mut positions = vec![pos(0.0, 0.0), pos(1.0, 1.0)];
        let before = positions.clone();
        let mut roots = BTreeSet::new();
        roots.insert(1);

        // Taffy owns flex layout; the sibling-repositioning shortcut is skipped.
        reposition_clean_subtrees(&sd, &tree, &roots, &mut positions);
        assert_eq!(positions, before);
    }

    #[test]
    fn reposition_block_flow_siblings_stacks_clean_children_from_the_content_origin() {
        // body(0) is the parent; children 1 and 2 are clean, 3 is a grandchild.
        let sd = styled(Dom::create_body().with_child(Dom::create_div()), "");
        let parent_bp = box_props(
            edges(0.0, 0.0, 0.0, 0.0),
            edges(5.0, 5.0, 5.0, 5.0), // border — see the note below
            edges(3.0, 3.0, 3.0, 3.0), // padding
        );
        let tree = build_tree(
            vec![
                hot(
                    None,
                    Some(NodeId::ZERO),
                    Some(size(200.0, 200.0)),
                    &parent_bp,
                ),
                hot(Some(0), None, Some(size(200.0, 50.0)), &zero_box_props()),
                hot(Some(0), None, Some(size(200.0, 30.0)), &zero_box_props()),
                plain(Some(1)),
            ],
            warm_default(4),
            &[vec![1, 2], vec![3], vec![], vec![]],
        );
        let mut positions = vec![
            pos(10.0, 20.0), // parent (border-box origin)
            pos(0.0, 0.0),
            pos(0.0, 0.0),
            pos(0.0, 0.0),
        ];

        reposition_block_flow_siblings(&sd, 0, &tree, &BTreeSet::new(), &mut positions);

        // BEHAVIOUR PIN: the content origin is computed as parent_pos + PADDING
        // only — the parent's border is NOT added, unlike calculate_content_box_pos
        // (which adds border + padding to the same border-box origin). With a
        // bordered parent the clean siblings therefore land 5px up/left of where
        // the full layout pass would put them.
        assert_eq!(positions[1], pos(13.0, 23.0));
        assert_eq!(
            positions[2],
            pos(13.0, 73.0),
            "second child stacked below the first"
        );
        // The grandchild moved with its parent's subtree.
        assert_eq!(positions[3], pos(13.0, 23.0));
    }

    // ==================================================================
    // compute_counters
    // ==================================================================

    #[test]
    fn compute_counters_on_an_empty_or_anonymous_tree_does_not_panic() {
        let sd = styled(Dom::create_body(), "");
        let mut counters: HashMap<(usize, String), i32> = HashMap::new();

        // Root index past the end of the node list.
        let empty = build_tree(Vec::new(), Vec::new(), &[]);
        compute_counters(&sd, &empty, &mut counters);
        assert!(counters.is_empty());

        // Anonymous root (no dom_node_id) with an anonymous child.
        let anon = build_tree(
            vec![plain(None), plain(Some(0))],
            warm_default(2),
            &[vec![1], vec![]],
        );
        compute_counters(&sd, &anon, &mut counters);
        assert!(counters.is_empty());
    }

    #[test]
    fn compute_counters_increments_the_list_item_counter_in_document_order() {
        // body(0) > [ div.li(1), div.li(2) ]
        let sd = styled(
            Dom::create_body()
                .with_child(div_class("li"))
                .with_child(div_class("li")),
            ".li { display: list-item; }",
        );
        let tree = build_tree(
            vec![
                hot(
                    None,
                    Some(NodeId::ZERO),
                    Some(size(100.0, 100.0)),
                    &zero_box_props(),
                ),
                hot(
                    Some(0),
                    Some(NodeId::new(1)),
                    Some(size(100.0, 10.0)),
                    &zero_box_props(),
                ),
                hot(
                    Some(0),
                    Some(NodeId::new(2)),
                    Some(size(100.0, 10.0)),
                    &zero_box_props(),
                ),
            ],
            warm_default(3),
            &[vec![1, 2], vec![], vec![]],
        );

        let mut counters: HashMap<(usize, String), i32> = HashMap::new();
        compute_counters(&sd, &tree, &mut counters);

        // CSS Lists 3 § 3: `display: list-item` auto-increments "list-item".
        assert_eq!(counters.get(&(1, "list-item".to_string())), Some(&1));
        assert_eq!(counters.get(&(2, "list-item".to_string())), Some(&2));
        // The non-list-item root never enters a counter scope.
        assert_eq!(counters.get(&(0, "list-item".to_string())), None);
    }

    #[test]
    fn compute_counters_leaves_plain_elements_alone() {
        let sd = styled(Dom::create_body().with_child(Dom::create_div()), "");
        let tree = build_tree(
            vec![
                hot(
                    None,
                    Some(NodeId::ZERO),
                    Some(size(100.0, 100.0)),
                    &zero_box_props(),
                ),
                hot(
                    Some(0),
                    Some(NodeId::new(1)),
                    Some(size(100.0, 10.0)),
                    &zero_box_props(),
                ),
            ],
            warm_default(2),
            &[vec![1], vec![]],
        );

        let mut counters: HashMap<(usize, String), i32> = HashMap::new();
        compute_counters(&sd, &tree, &mut counters);
        assert!(
            counters.is_empty(),
            "no counter-reset/increment → no counters"
        );
    }
}
